use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use satchel_core::{Client, DeviceId, Follow, Location, init, restore_latest};
use tempfile::tempdir;

#[test]
fn recovery_password_restores_the_same_bytes_as_official_restic() {
    let workspace = tempdir().expect("temp dir");
    let repository = workspace.path().join("repository");
    let work = workspace.path().join("work");
    let via_core = workspace.path().join("core");
    let via_restic = workspace.path().join("restic");
    let cache = workspace.path().join("cache");
    fs::create_dir_all(&repository).expect("repository");
    fs::create_dir_all(&cache).expect("cache");

    let device_id = DeviceId::parse("0123456789abcdef0123456789abcdef").expect("device");
    let password = "recovery-password";
    let location = Location::Directory(repository.clone());
    init(&location, password, &device_id).expect("init");

    let mut client = Client::open(device_id, &work, password).expect("client");
    fs::write(work.join("kept.txt"), b"kept-bytes").expect("file");
    client.follow("kept.txt", Follow::Always).expect("follow");
    client.set_clock(1_700_000_100).expect("clock");
    client.sync(&location).expect("sync");
    drop(client);

    restore_latest(&location, password, &via_core).expect("core restore");

    let output = Command::new(restic_binary())
        .env("RESTIC_PASSWORD", password)
        .env("RESTIC_CACHE_DIR", &cache)
        .args([
            "-r",
            repository.to_str().expect("repo path"),
            "restore",
            "latest",
            "--target",
            via_restic.to_str().expect("restic target"),
        ])
        .output()
        .expect("run restic");
    assert!(
        output.status.success(),
        "restic restore failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let core_bytes = fs::read(find_named(&via_core, "kept.txt")).expect("core file");
    let restic_bytes = fs::read(find_named(&via_restic, "kept.txt")).expect("restic file");
    assert_eq!(core_bytes, b"kept-bytes");
    assert_eq!(restic_bytes, core_bytes);
}

fn find_named(root: &Path, name: &str) -> PathBuf {
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(&dir).expect("read dir") {
            let entry = entry.expect("entry");
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
                continue;
            }
            if path.file_name().and_then(|value| value.to_str()) == Some(name) {
                return path;
            }
        }
    }
    panic!("missing {name}");
}

fn restic_binary() -> PathBuf {
    if let Some(path) = std::env::var_os("RESTIC_BIN") {
        return PathBuf::from(path);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.satchel/bin/restic")
}
