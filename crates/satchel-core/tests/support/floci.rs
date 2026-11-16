//! Local S3 emulator for tests: Floci in a loopback-only container, removed on teardown.
//! Shared by the core and desktop test binaries.
#![allow(dead_code)]

use std::collections::HashMap;
use std::fs;
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::OnceLock;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use testcontainers::bollard::models::PortBinding;
use testcontainers::core::{IntoContainerPort, WaitFor};
use testcontainers::runners::SyncRunner;
use testcontainers::{GenericImage, ImageExt};

pub const ACCESS_KEY: &str = "test";
pub const SECRET_KEY: &str = "test";

pub fn aws(endpoint: &str, args: &[&str]) {
    let output = aws_output(endpoint, args);
    assert!(output.status.success(), "aws status {}", output.status);
}

pub fn aws_stdout(endpoint: &str, args: &[&str]) -> String {
    let output = aws_output(endpoint, args);
    assert!(output.status.success(), "aws status {}", output.status);
    String::from_utf8(output.stdout).expect("aws utf8")
}

pub fn aws_to_file(endpoint: &str, args: &[&str], dest: &Path) {
    let mut command = aws_command(endpoint, args);
    command.arg(dest);
    let output = command.output().expect("aws");
    assert!(output.status.success(), "aws status {}", output.status);
}

pub fn aws_output(endpoint: &str, args: &[&str]) -> Output {
    aws_command(endpoint, args).output().expect("aws")
}

pub fn aws_command(endpoint: &str, args: &[&str]) -> Command {
    let mut command = Command::new("aws");
    command.args(args);
    command.arg("--endpoint-url");
    command.arg(endpoint);
    command.arg("--region");
    command.arg("us-east-1");
    command.env("AWS_ACCESS_KEY_ID", ACCESS_KEY);
    command.env("AWS_SECRET_ACCESS_KEY", SECRET_KEY);
    command.env("AWS_DEFAULT_REGION", "us-east-1");
    command.env("AWS_CONFIG_FILE", "/dev/null");
    command.env("AWS_SHARED_CREDENTIALS_FILE", "/dev/null");
    command.env_remove("AWS_PROFILE");
    command.env_remove("AWS_DEFAULT_PROFILE");
    command.env_remove("AWS_SESSION_TOKEN");
    command.env_remove("AWS_SECURITY_TOKEN");
    command
}

pub struct LocalFloci {
    endpoint: String,
    container: testcontainers::Container<GenericImage>,
}

impl LocalFloci {
    pub fn start() -> Self {
        use_local_docker();
        let image_id = require_local_image();
        emit_proof("floci image floci/floci:2.3.0 already local, no image pull");
        remove_stale_containers();
        let name = format!("satchel-floci-{}", std::process::id());
        let container = GenericImage::new("floci/floci", "2.3.0")
            .with_exposed_port(4566.tcp())
            .with_wait_for(WaitFor::message_on_stdout("Ready."))
            .with_container_name(name)
            .with_host_config_modifier(|host| {
                host.publish_all_ports = Some(false);
                let mut bindings = HashMap::new();
                bindings.insert(
                    "4566/tcp".to_string(),
                    Some(vec![PortBinding {
                        host_ip: Some("127.0.0.1".to_string()),
                        host_port: Some("0".to_string()),
                    }]),
                );
                host.port_bindings = Some(bindings);
            })
            .start()
            .expect("start Floci");
        assert_eq!(
            image_id,
            require_local_image(),
            "local image changed while the container started"
        );
        assert_loopback_only(container.id());
        let port = container
            .get_host_port_ipv4(4566.tcp())
            .expect("floci port");
        wait_until_accepts(port);
        let endpoint = format!("http://127.0.0.1:{port}");
        emit_proof(&format!("floci container start {endpoint}"));
        Self {
            endpoint,
            container,
        }
    }

    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }
}

impl Drop for LocalFloci {
    fn drop(&mut self) {
        let id = self.container.id().to_string();
        let volumes = anonymous_volume_names(&id);
        let removed = docker()
            .args(["rm", "-f", "-v", id.as_str()])
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
            && container_gone(&id)
            && volumes.as_ref().is_some_and(|names| volumes_gone(names));
        if removed {
            emit_proof("floci container removed");
            return;
        }
        if !std::thread::panicking() {
            panic!("floci container was not removed");
        }
    }
}

/// libtest hides `println` on success. The proof has to reach the process
/// stderr that `cargo test` forwards into its log.
pub fn emit_proof(line: &str) {
    use std::io::Write;
    let mut stderr = fs::OpenOptions::new()
        .write(true)
        .open("/dev/stderr")
        .expect("stderr");
    writeln!(stderr, "{line}").expect("proof line");
}

pub static DOCKER_CONTEXT: OnceLock<String> = OnceLock::new();

pub fn docker() -> Command {
    let mut command = Command::new("docker");
    if let Some(context) = DOCKER_CONTEXT.get() {
        command.arg("--context");
        command.arg(context);
    }
    command
}

pub fn require_local_image() -> String {
    let output = docker()
        .args([
            "image",
            "inspect",
            "--format",
            "{{.Id}}",
            "floci/floci:2.3.0",
        ])
        .output()
        .expect("docker");
    assert!(
        output.status.success(),
        "floci/floci:2.3.0 is not present locally"
    );
    let id = String::from_utf8(output.stdout).expect("image id");
    let id = id.trim();
    assert!(!id.is_empty(), "image id is empty");
    id.to_string()
}

pub fn anonymous_volume_names(container_id: &str) -> Option<Vec<String>> {
    let output = docker()
        .args([
            "inspect",
            "--format",
            "{{range .Mounts}}{{if eq .Type \"volume\"}}{{.Name}}\n{{end}}{{end}}",
            container_id,
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?;
    let mut names = Vec::new();
    for line in text.lines() {
        let name = line.trim();
        if name.is_empty() {
            continue;
        }
        names.push(name.to_string());
    }
    Some(names)
}

pub fn container_gone(container_id: &str) -> bool {
    match docker().args(["inspect", container_id]).output() {
        Ok(output) => !output.status.success(),
        Err(_) => false,
    }
}

pub fn volumes_gone(names: &[String]) -> bool {
    for name in names {
        let gone = match docker().args(["volume", "inspect", name]).output() {
            Ok(output) => !output.status.success(),
            Err(_) => false,
        };
        if !gone {
            return false;
        }
    }
    true
}

pub fn remove_stale_containers() {
    let listed = docker()
        .args(["ps", "-aq", "--filter", "name=satchel-floci-"])
        .output()
        .expect("docker ps");
    if !listed.status.success() {
        return;
    }
    let text = String::from_utf8(listed.stdout).unwrap_or_default();
    for id in text.split_whitespace() {
        let _ = docker().args(["rm", "-f", "-v", id]).output();
    }
}

/// A Docker endpoint this test may use. Remote schemes are rejected.
pub fn local_socket_host(host: &str) -> bool {
    host.starts_with("unix://")
}

pub fn context_endpoint(name: &str) -> Option<String> {
    let output = Command::new("docker")
        .args([
            "context",
            "inspect",
            name,
            "--format",
            "{{.Endpoints.docker.Host}}",
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let host = String::from_utf8(output.stdout).ok()?;
    let host = host.trim();
    if host.is_empty() {
        return None;
    }
    Some(host.to_string())
}

pub fn current_context() -> String {
    let output = Command::new("docker")
        .args(["context", "show"])
        .output()
        .expect("docker context");
    assert!(
        output.status.success(),
        "docker context status {}",
        output.status
    );
    let name = String::from_utf8(output.stdout).expect("context name");
    let name = name.trim();
    assert!(!name.is_empty(), "docker context has no name");
    name.to_string()
}

/// Prefer a real rootless daemon. A context that is only named `rootless` is ignored.
pub fn verified_rootless_context() -> Option<String> {
    let host = context_endpoint("rootless")?;
    if !local_socket_host(&host) {
        return None;
    }
    let output = Command::new("docker")
        .args([
            "--context",
            "rootless",
            "info",
            "--format",
            "{{range .SecurityOptions}}{{println .}}{{end}}",
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let options = String::from_utf8(output.stdout).unwrap_or_default();
    if !options
        .lines()
        .any(|line| line.starts_with("name=rootless"))
    {
        return None;
    }
    Some("rootless".to_string())
}

pub fn use_local_docker() {
    let context = if let Some(name) = verified_rootless_context() {
        name
    } else {
        current_context()
    };
    let host = context_endpoint(&context).expect("docker context endpoint");
    assert!(
        local_socket_host(&host),
        "docker host is not a local socket"
    );
    let _ = DOCKER_CONTEXT.set(context);
    // SAFETY: this runs before testcontainers starts its runtime. The socket
    // path is not printed. Later Docker commands in this process use the same
    // context.
    unsafe {
        std::env::set_var("DOCKER_HOST", host);
    }
}

pub fn assert_loopback_only(container_id: &str) {
    let output = docker()
        .args([
            "inspect",
            "--format",
            "{{.HostConfig.NetworkMode}}\n{{range .Mounts}}{{.Type}}\n{{end}}{{range $p, $conf := .NetworkSettings.Ports}}{{range $conf}}{{.HostIp}}\n{{end}}{{end}}",
            container_id,
        ])
        .output()
        .expect("docker inspect");
    assert!(output.status.success(), "inspect status {}", output.status);
    let text = String::from_utf8(output.stdout).expect("inspect utf8");
    let mut lines = text.lines();
    let network = lines.next().unwrap_or("");
    assert_ne!(network, "host");
    let mut saw_loopback = false;
    for line in lines {
        let value = line.trim();
        if value.is_empty() {
            continue;
        }
        if value == "volume" {
            continue;
        }
        assert_ne!(value, "bind", "container bind-mounts the host");
        assert_eq!(value, "127.0.0.1");
        saw_loopback = true;
    }
    assert!(saw_loopback, "container published no loopback port");
}

pub fn wait_until_accepts(port: u16) {
    let address = format!("127.0.0.1:{port}");
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline {
        if TcpStream::connect(&address).is_ok() {
            return;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    panic!("Floci did not accept connections");
}

pub struct TempRoot(pub PathBuf);

impl TempRoot {
    pub fn new() -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        Self(std::env::temp_dir().join(format!("satchel-floci-{nanos}")))
    }
}

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
