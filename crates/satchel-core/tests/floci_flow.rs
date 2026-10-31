//! Desktop and phone share one repository on a local S3 emulator.
//!
//! `cargo test` starts `floci/floci:2.3.0` through testcontainers, publishes
//! port 4566 on `127.0.0.1` only, and removes that container on teardown.
//! The image must already be local. This test does not read `FLOCI_ENDPOINT`.

use std::collections::HashMap;
use std::fs;
use std::net::TcpStream;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use satchel_core::{
    Client, DeviceId, Follow, Location, S3Store, SetupCheck, StorageKind, Versioning, check_setup,
    init, read_version,
};
use testcontainers::bollard::models::PortBinding;
use testcontainers::core::{IntoContainerPort, WaitFor};
use testcontainers::runners::SyncRunner;
use testcontainers::{GenericImage, ImageExt};

const DESKTOP: &str = "0123456789abcdef0123456789abcdef";
const PHONE: &str = "fedcba9876543210fedcba9876543210";
const PASSWORD: &str = "recovery-password";
const ACCESS_KEY: &str = "test";
const SECRET_KEY: &str = "test";

#[test]
fn desktop_publishes_and_the_phone_reads_metadata_only() {
    let floci = LocalFloci::start();
    let endpoint = floci.endpoint();
    let bucket = format!("satchel-flow-{}", std::process::id());
    aws(
        endpoint,
        &["s3api", "create-bucket", "--bucket", bucket.as_str()],
    );
    aws(
        endpoint,
        &[
            "s3api",
            "put-bucket-versioning",
            "--bucket",
            bucket.as_str(),
            "--versioning-configuration",
            "Status=Enabled",
        ],
    );
    let versioning = aws_stdout(
        endpoint,
        &[
            "s3api",
            "get-bucket-versioning",
            "--bucket",
            bucket.as_str(),
        ],
    );
    assert!(versioning.contains("Enabled"));
    let lifecycle = aws_output(
        endpoint,
        &[
            "s3api",
            "get-bucket-lifecycle-configuration",
            "--bucket",
            bucket.as_str(),
        ],
    );
    assert!(
        !lifecycle.status.success(),
        "lifecycle status {}",
        lifecycle.status
    );
    let gate = check_setup(StorageKind::S3, Some(Versioning::Enabled), false).expect("gate");
    assert_eq!(gate, SetupCheck::Ready);

    let location = s3_location(&bucket, endpoint);
    let root = TempRoot::new();
    let source = root.0.join("desktop");
    fs::create_dir_all(source.join("notes")).expect("source");
    fs::write(source.join("notes/from-desktop.txt"), "desk-bytes").expect("file");
    let desktop_id = DeviceId::parse(DESKTOP).expect("desktop id");
    init(&location, PASSWORD, &desktop_id).expect("init");
    let mut desktop = Client::open(desktop_id, &source, PASSWORD).expect("desktop");
    desktop
        .follow("notes/from-desktop.txt", Follow::Always)
        .expect("follow");
    desktop.set_clock(1_700_000_000).expect("clock");
    desktop.sync(&location).expect("desktop sync");

    let phone_work = root.0.join("phone");
    let phone_id = DeviceId::parse(PHONE).expect("phone id");
    let phone = Client::open(phone_id, &phone_work, PASSWORD).expect("phone");
    let catalog = phone.catalog(&location).expect("catalog");
    let row = catalog
        .iter()
        .find(|entry| entry.path == "notes/from-desktop.txt")
        .expect("metadata");
    assert_eq!(row.follow, Follow::See);
    assert_eq!(row.devices, vec![DESKTOP.to_string()]);
    assert!(
        !phone_work.join("notes/from-desktop.txt").exists(),
        "first look wrote the work copy"
    );
    let bytes = read_version(
        &location,
        PASSWORD,
        &row_snapshot(&location),
        "notes/from-desktop.txt",
    )
    .expect("read");
    assert_eq!(bytes, b"desk-bytes");
    assert!(!phone_work.join("notes/from-desktop.txt").exists());

    assert_key_names(endpoint, &bucket, &root.0.join("key.json"), DESKTOP);
    let restored = root.0.join("restic-restore");
    restore_with_official_restic(endpoint, &bucket, &root.0.join("restic-cache"), &restored);
    let restored_bytes =
        fs::read(restored.join("satchel/notes/from-desktop.txt")).expect("restored");
    assert_eq!(restored_bytes, b"desk-bytes");

    if let Ok(path) = std::env::var("FLOCI_PHONE_SPEC")
        && !path.is_empty()
    {
        write_phone_spec(Path::new(&path), &bucket, endpoint);
    }
}

fn row_snapshot(location: &Location) -> String {
    let listed = satchel_core::cursors(location, PASSWORD).expect("cursors");
    let mut chosen: Option<&satchel_core::Cursor> = None;
    for cursor in &listed {
        let replace = match chosen {
            None => true,
            Some(current) => cursor.time_unix > current.time_unix,
        };
        if replace {
            chosen = Some(cursor);
        }
    }
    chosen.expect("snapshot").snapshot_id.clone()
}

fn s3_location(bucket: &str, endpoint: &str) -> Location {
    Location::S3(
        S3Store::new(
            bucket,
            endpoint,
            "us-east-1",
            "repo",
            ACCESS_KEY,
            SECRET_KEY,
        )
        .expect("store"),
    )
}

fn write_phone_spec(path: &Path, bucket: &str, endpoint: &str) {
    let phone_endpoint = endpoint.replace("127.0.0.1", "10.0.2.2");
    let body = serde_json::json!({
        "bucket": bucket,
        "endpoint": phone_endpoint,
        "region": "us-east-1",
        "prefix": "repo",
        "access_key_id": ACCESS_KEY,
        "secret_access_key": SECRET_KEY,
        "password": PASSWORD,
        "work": "/data/user/0/io.github.lucaswilliameufrasio.satchel/cache/shared-work",
    });
    let text = serde_json::to_string(&body).expect("spec");
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("spec dir");
    }
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
        .expect("spec file");
    std::io::Write::write_all(&mut file, text.as_bytes()).expect("spec write");
}

fn assert_key_names(endpoint: &str, bucket: &str, dest: &Path, device_id: &str) {
    let listed = aws_stdout(
        endpoint,
        &[
            "s3api",
            "list-objects-v2",
            "--bucket",
            bucket,
            "--prefix",
            "repo/keys/",
            "--query",
            "Contents[].Key",
            "--output",
            "text",
        ],
    );
    let prefix = "repo/keys/";
    let mut object_key = "";
    for part in listed.split_whitespace() {
        if part.starts_with(prefix) && part.len() > prefix.len() {
            object_key = part;
            break;
        }
    }
    assert!(!object_key.is_empty(), "missing key file");
    aws_to_file(
        endpoint,
        &[
            "s3api",
            "get-object",
            "--bucket",
            bucket,
            "--key",
            object_key,
        ],
        dest,
    );
    let text = fs::read_to_string(dest).expect("key json");
    let value: serde_json::Value = serde_json::from_str(&text).expect("parse key");
    assert_eq!(value["hostname"].as_str(), Some(device_id));
    assert_eq!(value["username"].as_str(), Some(device_id));
}

fn restore_with_official_restic(endpoint: &str, bucket: &str, cache: &Path, restore: &Path) {
    fs::create_dir_all(cache).expect("restic cache");
    let repository = format!("s3:{endpoint}/{bucket}/repo");
    let mut command = Command::new(restic_binary());
    command.env("RESTIC_PASSWORD", PASSWORD);
    command.env("RESTIC_CACHE_DIR", cache);
    command.env("AWS_ACCESS_KEY_ID", ACCESS_KEY);
    command.env("AWS_SECRET_ACCESS_KEY", SECRET_KEY);
    command.env("AWS_DEFAULT_REGION", "us-east-1");
    command.env("AWS_CONFIG_FILE", "/dev/null");
    command.env("AWS_SHARED_CREDENTIALS_FILE", "/dev/null");
    command.env_remove("AWS_PROFILE");
    command.env_remove("AWS_DEFAULT_PROFILE");
    command.env_remove("AWS_SESSION_TOKEN");
    command.env_remove("AWS_SECURITY_TOKEN");
    command.args([
        "-o",
        "s3.bucket-lookup=path",
        "-r",
        repository.as_str(),
        "restore",
        "latest",
        "--target",
    ]);
    command.arg(restore);
    let output = command.output().expect("restic");
    assert!(output.status.success(), "restic status {}", output.status);
}

fn restic_binary() -> PathBuf {
    if let Some(path) = std::env::var_os("RESTIC_BIN") {
        return PathBuf::from(path);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.satchel/bin/restic")
}

fn aws(endpoint: &str, args: &[&str]) {
    let output = aws_output(endpoint, args);
    assert!(output.status.success(), "aws status {}", output.status);
}

fn aws_stdout(endpoint: &str, args: &[&str]) -> String {
    let output = aws_output(endpoint, args);
    assert!(output.status.success(), "aws status {}", output.status);
    String::from_utf8(output.stdout).expect("aws utf8")
}

fn aws_to_file(endpoint: &str, args: &[&str], dest: &Path) {
    let mut command = aws_command(endpoint, args);
    command.arg(dest);
    let output = command.output().expect("aws");
    assert!(output.status.success(), "aws status {}", output.status);
}

fn aws_output(endpoint: &str, args: &[&str]) -> Output {
    aws_command(endpoint, args).output().expect("aws")
}

fn aws_command(endpoint: &str, args: &[&str]) -> Command {
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

struct LocalFloci {
    endpoint: String,
    container: testcontainers::Container<GenericImage>,
}

impl LocalFloci {
    fn start() -> Self {
        use_rootless_docker();
        require_local_image();
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
        assert_loopback_only(container.id());
        let port = container
            .get_host_port_ipv4(4566.tcp())
            .expect("floci port");
        wait_until_accepts(port);
        Self {
            endpoint: format!("http://127.0.0.1:{port}"),
            container,
        }
    }

    fn endpoint(&self) -> &str {
        &self.endpoint
    }
}

impl Drop for LocalFloci {
    fn drop(&mut self) {
        let id = self.container.id().to_string();
        let _ = Command::new("docker")
            .args(["--context", "rootless", "rm", "-f", "-v", id.as_str()])
            .output();
    }
}

fn require_local_image() {
    let output = Command::new("docker")
        .args([
            "--context",
            "rootless",
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
}

fn remove_stale_containers() {
    let listed = Command::new("docker")
        .args([
            "--context",
            "rootless",
            "ps",
            "-aq",
            "--filter",
            "name=satchel-floci-",
        ])
        .output()
        .expect("docker ps");
    if !listed.status.success() {
        return;
    }
    let text = String::from_utf8(listed.stdout).unwrap_or_default();
    for id in text.split_whitespace() {
        let _ = Command::new("docker")
            .args(["--context", "rootless", "rm", "-f", "-v", id])
            .output();
    }
}

fn use_rootless_docker() {
    let output = Command::new("docker")
        .args([
            "context",
            "inspect",
            "rootless",
            "--format",
            "{{.Endpoints.docker.Host}}",
        ])
        .output()
        .expect("docker context");
    assert!(
        output.status.success(),
        "rootless context status {}",
        output.status
    );
    let host = String::from_utf8(output.stdout).expect("docker host utf8");
    let host = host.trim();
    assert!(
        host.starts_with("unix://"),
        "rootless context is not a local socket"
    );
    // SAFETY: this runs before testcontainers starts its runtime. The value is
    // the rootless socket and is not printed. Later Docker commands in this
    // process must reach that same daemon.
    unsafe {
        std::env::set_var("DOCKER_HOST", host);
    }
    let info = Command::new("docker")
        .args([
            "info",
            "--format",
            "{{range .SecurityOptions}}{{println .}}{{end}}",
        ])
        .output()
        .expect("docker info");
    assert!(info.status.success(), "docker info status {}", info.status);
    let options = String::from_utf8(info.stdout).expect("security options");
    assert!(
        options
            .lines()
            .any(|line| line.starts_with("name=rootless")),
        "docker daemon is not rootless"
    );
}

fn assert_loopback_only(container_id: &str) {
    let output = Command::new("docker")
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

fn wait_until_accepts(port: u16) {
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

struct TempRoot(PathBuf);

impl TempRoot {
    fn new() -> Self {
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
