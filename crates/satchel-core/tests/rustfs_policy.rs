//! The phone credential policy, enforced by a store that applies identity policy.
//!
//! Floci has no IAM, so it cannot show that the limited key is limited. RustFS
//! applies IAM policies, so this starts a throwaway RustFS on loopback, attaches
//! `phone_credential_policy` to a user, and checks what that user may delete.
//! It skips when the images are not already local (`rustfs/rustfs`, `rustfs/rc`).

#[path = "support/floci.rs"]
mod floci;

use std::process::Stdio;

use floci::{docker, emit_proof, use_local_docker, wait_until_accepts};
use satchel_core::phone_credential_policy;

const ADMIN: &str = "rustfsadmin";
const ADMIN_SECRET: &str = "rustfsadmin-secret-1";
const PHONE: &str = "phoneuser";
const PHONE_SECRET: &str = "phone-secret-12345";
const LOCK: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn image_present(name: &str) -> bool {
    docker()
        .args(["image", "inspect", name])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

struct RustFs {
    id: String,
    port: u16,
}

impl RustFs {
    fn start() -> Self {
        let output = docker()
            .args([
                "run",
                "-d",
                "-e",
                &format!("RUSTFS_ACCESS_KEY={ADMIN}"),
                "-e",
                &format!("RUSTFS_SECRET_KEY={ADMIN_SECRET}"),
                "-p",
                "127.0.0.1::9000",
                "rustfs/rustfs:latest",
            ])
            .output()
            .expect("docker run");
        assert!(output.status.success(), "rustfs did not start");
        let id = String::from_utf8(output.stdout)
            .expect("id")
            .trim()
            .to_string();
        let port = docker()
            .args(["port", &id, "9000/tcp"])
            .output()
            .expect("docker port");
        let text = String::from_utf8(port.stdout).expect("port text");
        let port: u16 = text
            .lines()
            .next()
            .and_then(|line| line.rsplit(':').next())
            .and_then(|digits| digits.trim().parse().ok())
            .expect("host port");
        wait_until_accepts(port);
        std::thread::sleep(std::time::Duration::from_secs(3));
        Self { id, port }
    }

    /// Run a shell script in the `rc` image, sharing this container's network.
    fn script(&self, dir: &std::path::Path, script: &str) -> String {
        let output = docker()
            .args([
                "run",
                "--rm",
                "--network",
                &format!("container:{}", self.id),
                "-v",
                &format!("{}:/w", dir.display()),
                "--entrypoint",
                "sh",
                "rustfs/rc:latest",
                "-c",
                script,
            ])
            .output()
            .expect("docker run rc");
        format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    }
}

impl Drop for RustFs {
    fn drop(&mut self) {
        let _ = docker().args(["rm", "-f", "-v", &self.id]).output();
        let _ = self.port;
    }
}

fn verdict(log: &str, name: &str) -> i32 {
    let marker = format!("RESULT {name}=");
    log.lines()
        .find_map(|line| line.strip_prefix(&marker))
        .and_then(|rest| rest.trim().parse().ok())
        .unwrap_or_else(|| panic!("no result for {name}"))
}

#[test]
fn the_phone_key_deletes_its_locks_and_nothing_else() {
    if !image_present("rustfs/rustfs:latest") || !image_present("rustfs/rc:latest") {
        eprintln!("SKIP: rustfs/rustfs and rustfs/rc are not local");
        return;
    }
    use_local_docker();
    let server = RustFs::start();
    let dir = std::env::temp_dir().join(format!("satchel-rustfs-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("dir");
    let policy = phone_credential_policy("satchel", "repo").expect("policy");
    std::fs::write(dir.join("policy.json"), policy).expect("write policy");

    let setup = format!(
        "set -e; \
         rc alias set adm http://127.0.0.1:9000 {ADMIN} {ADMIN_SECRET} >/dev/null; \
         rc mb adm/satchel >/dev/null; rc version enable adm/satchel >/dev/null; \
         rc admin policy create adm phone /w/policy.json >/dev/null; \
         rc admin user add adm {PHONE} {PHONE_SECRET} >/dev/null; \
         rc admin policy attach adm phone --user {PHONE} >/dev/null; echo ready"
    );
    let log = server.script(&dir, &setup);
    assert!(log.contains("ready"), "setup failed: {log}");

    let probe = format!(
        "rc alias set phn http://127.0.0.1:9000 {PHONE} {PHONE_SECRET} >/dev/null; \
         echo x > /tmp/x; \
         r() {{ n=$1; shift; \"$@\" >/dev/null 2>&1; echo \"RESULT $n=$?\"; }}; \
         r put_pack rc cp /tmp/x phn/satchel/repo/data/aa/pack1; \
         r put_lock rc cp /tmp/x phn/satchel/repo/locks/{LOCK}; \
         r put_key rc cp /tmp/x phn/satchel/repo/keys/k1; \
         r list rc ls phn/satchel/repo/ --recursive; \
         r delete_lock rc rm phn/satchel/repo/locks/{LOCK}; \
         r delete_pack rc rm phn/satchel/repo/data/aa/pack1; \
         r delete_key rc rm phn/satchel/repo/keys/k1; \
         r put_outside rc cp /tmp/x phn/satchel/other/file; \
         r suspend_versioning rc version suspend phn/satchel; \
         r delete_bucket rc rb phn/satchel; \
         r admin_lists_users rc admin user ls phn"
    );
    let log = server.script(&dir, &probe);
    emit_proof(&format!("rustfs phone key results:\n{log}"));
    assert_eq!(verdict(&log, "put_pack"), 0, "phone writes packs");
    assert_eq!(verdict(&log, "put_lock"), 0, "phone writes locks");
    assert_eq!(verdict(&log, "list"), 0, "phone lists");
    assert_eq!(verdict(&log, "delete_lock"), 0, "phone deletes its lock");
    assert_ne!(
        verdict(&log, "delete_pack"),
        0,
        "phone must not delete packs"
    );
    assert_ne!(verdict(&log, "delete_key"), 0, "phone must not delete keys");
    assert_ne!(
        verdict(&log, "put_outside"),
        0,
        "phone stays inside the prefix"
    );
    assert_ne!(
        verdict(&log, "suspend_versioning"),
        0,
        "phone must not touch versioning"
    );
    assert_ne!(
        verdict(&log, "delete_bucket"),
        0,
        "phone must not delete the bucket"
    );
    assert_ne!(
        verdict(&log, "admin_lists_users"),
        0,
        "phone is not an admin"
    );

    // The pack the phone could not delete is still there for the maintainer.
    let check = server.script(
        &dir,
        &format!(
            "rc alias set adm http://127.0.0.1:9000 {ADMIN} {ADMIN_SECRET} >/dev/null; \
             rc ls adm/satchel/repo/ --recursive"
        ),
    );
    assert!(
        check.contains("pack1") && check.contains("k1"),
        "history kept: {check}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
