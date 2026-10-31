use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use satchel_core::{
    Client, DeviceId, Error, Follow, Location, add_device_key, init, remove_device_key,
    restore_latest,
};
use tempfile::tempdir;

const HOLDER: &str = "0123456789abcdef0123456789abcdef";
const PHONE: &str = "fedcba9876543210fedcba9876543210";

#[test]
fn device_key_opens_without_the_recovery_password() {
    let workspace = tempdir().expect("temp dir");
    let repository = workspace.path().join("repository");
    let work = workspace.path().join("work");
    let via_device = workspace.path().join("device");
    let via_restic = workspace.path().join("restic");
    let via_recovery = workspace.path().join("recovery");
    let cache = workspace.path().join("cache");
    fs::create_dir_all(&repository).expect("repository");
    fs::create_dir_all(&cache).expect("cache");

    let holder = DeviceId::parse(HOLDER).expect("holder");
    let phone = DeviceId::parse(PHONE).expect("phone");
    let recovery = "recovery-password";
    let device_password = "device-key-password";
    let location = Location::Directory(repository.clone());

    let empty = add_device_key(&location, recovery, &phone, "");
    assert!(matches!(empty, Err(Error::EmptyPassword)));

    init(&location, recovery, &holder).expect("init");
    let before = key_names(&repository);
    assert_eq!(before.len(), 1);
    let recovery_key = before[0].clone();

    let mut client = Client::open(holder.clone(), &work, recovery).expect("holder");
    fs::write(work.join("kept.txt"), b"kept-bytes").expect("file");
    client.follow("kept.txt", Follow::Always).expect("follow");
    client.set_clock(1_700_000_100).expect("clock");
    client.sync(&location).expect("sync");
    drop(client);

    let key_id = add_device_key(&location, recovery, &phone, device_password).expect("add key");
    assert_eq!(key_id.len(), 64);
    assert_ne!(key_id, recovery_key);
    let mut names = key_names(&repository);
    names.sort();
    assert!(names.contains(&key_id));
    let key_file = fs::read_to_string(repository.join("keys").join(&key_id)).expect("key file");
    let value: serde_json::Value = serde_json::from_str(&key_file).expect("key json");
    assert_eq!(
        value.get("hostname").and_then(|item| item.as_str()),
        Some(PHONE)
    );
    assert_eq!(
        value.get("username").and_then(|item| item.as_str()),
        Some(PHONE)
    );
    assert!(value.get("created").is_none());

    restore_latest(&location, device_password, &via_device).expect("device restore");
    let device_bytes = fs::read(find_named(&via_device, "kept.txt")).expect("device file");
    assert_eq!(device_bytes, b"kept-bytes");
    assert!(
        restic_restore(&repository, &cache, &via_restic, device_password),
        "official restic did not restore with the device key"
    );
    let restic_bytes = fs::read(find_named(&via_restic, "kept.txt")).expect("restic file");
    assert_eq!(restic_bytes, b"kept-bytes");

    let refused = remove_device_key(&location, recovery, &recovery_key);
    assert!(matches!(refused, Err(Error::CurrentKey)));
    let bad = remove_device_key(&location, recovery, "abc");
    assert!(matches!(bad, Err(Error::KeyId)));

    remove_device_key(&location, recovery, &key_id).expect("remove device key");
    assert_eq!(key_names(&repository), vec![recovery_key]);
    let gone = workspace.path().join("gone");
    assert!(restore_latest(&location, device_password, &gone).is_err());
    assert!(
        !restic_restore(
            &repository,
            &cache,
            &workspace.path().join("restic-gone"),
            device_password
        ),
        "official restic still opened with the removed device key"
    );
    restore_latest(&location, recovery, &via_recovery).expect("recovery still opens");
    let recovery_bytes = fs::read(find_named(&via_recovery, "kept.txt")).expect("recovery file");
    assert_eq!(recovery_bytes, b"kept-bytes");
}

fn key_names(repository: &Path) -> Vec<String> {
    let mut names = Vec::new();
    for entry in fs::read_dir(repository.join("keys")).expect("keys") {
        let entry = entry.expect("entry");
        if !entry.file_type().expect("type").is_file() {
            continue;
        }
        let name = entry.file_name();
        let Some(text) = name.to_str() else {
            continue;
        };
        if text.is_empty() {
            continue;
        }
        names.push(text.to_string());
    }
    names.sort();
    names
}

fn restic_restore(repository: &Path, cache: &Path, target: &Path, password: &str) -> bool {
    let output = Command::new(restic_binary())
        .env("RESTIC_PASSWORD", password)
        .env("RESTIC_CACHE_DIR", cache)
        .args([
            "-r",
            repository.to_str().expect("repo path"),
            "restore",
            "latest",
            "--target",
            target.to_str().expect("target"),
        ])
        .output()
        .expect("run restic");
    output.status.success()
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
