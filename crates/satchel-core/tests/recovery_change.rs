use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use satchel_core::{
    Client, DeviceId, Error, Follow, Location, add_device_key, change_recovery_password, init,
    restore_latest,
};
use tempfile::tempdir;

const HOLDER: &str = "0123456789abcdef0123456789abcdef";
const PHONE: &str = "fedcba9876543210fedcba9876543210";

#[test]
fn changing_the_recovery_password_keeps_devices_and_drops_the_old_one() {
    let workspace = tempdir().expect("temp dir");
    let repository = workspace.path().join("repository");
    let work = workspace.path().join("work");
    let cache = workspace.path().join("cache");
    fs::create_dir_all(&repository).expect("repository");
    fs::create_dir_all(&cache).expect("cache");
    let holder = DeviceId::parse(HOLDER).expect("holder");
    let phone = DeviceId::parse(PHONE).expect("phone");
    let location = Location::Directory(repository.clone());
    let (old, new, device_password) = ("old-recovery", "new-recovery", "device-key-password");

    init(&location, old, &holder).expect("init");
    let mut client = Client::open(holder.clone(), &work, old).expect("holder");
    fs::write(work.join("kept.txt"), b"kept-bytes").expect("file");
    client.follow("kept.txt", Follow::Always).expect("follow");
    client.set_clock(1_700_000_100).expect("clock");
    client.sync(&location).expect("sync");
    drop(client);
    let phone_key = add_device_key(&location, old, &phone, device_password).expect("phone key");
    assert_eq!(key_names(&repository).len(), 2);

    assert!(matches!(
        change_recovery_password(&location, old, old),
        Err(Error::SamePassword)
    ));
    assert!(matches!(
        change_recovery_password(&location, old, ""),
        Err(Error::EmptyPassword)
    ));
    assert!(change_recovery_password(&location, "wrong", new).is_err());
    assert_eq!(
        key_names(&repository).len(),
        2,
        "a refused change writes nothing"
    );

    let new_id = change_recovery_password(&location, old, new).expect("change");
    assert_eq!(new_id.len(), 64);
    let names = key_names(&repository);
    assert_eq!(names.len(), 2);
    assert!(names.contains(&new_id) && names.contains(&phone_key));

    // The new key file stays opaque: the holder's id, no created.
    let file = fs::read_to_string(repository.join("keys").join(&new_id)).expect("key file");
    let value: serde_json::Value = serde_json::from_str(&file).expect("json");
    assert_eq!(value["hostname"].as_str(), Some(HOLDER));
    assert_eq!(value["username"].as_str(), Some(HOLDER));
    assert!(value.get("created").is_none());

    // New password and the phone's key open; the old password does not, here or in restic.
    restore_latest(&location, new, &workspace.path().join("a")).expect("new opens");
    restore_latest(&location, device_password, &workspace.path().join("b")).expect("phone opens");
    assert!(restore_latest(&location, old, &workspace.path().join("c")).is_err());
    assert!(restic_restore(
        &repository,
        &cache,
        &workspace.path().join("d"),
        new
    ));
    assert!(!restic_restore(
        &repository,
        &cache,
        &workspace.path().join("e"),
        old
    ));
    let bytes = fs::read(find_named(&workspace.path().join("d"), "kept.txt")).expect("file");
    assert_eq!(bytes, b"kept-bytes");
}

fn key_names(repository: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(repository.join("keys"))
        .expect("keys")
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry
                .file_type()
                .map(|kind| kind.is_file())
                .unwrap_or(false)
        })
        .filter_map(|entry| entry.file_name().to_str().map(str::to_string))
        .collect();
    names.sort();
    names
}

fn restic_restore(repository: &Path, cache: &Path, target: &Path, password: &str) -> bool {
    let binary = std::env::var_os("RESTIC_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.satchel/bin/restic")
        });
    Command::new(binary)
        .env("RESTIC_PASSWORD", password)
        .env("RESTIC_CACHE_DIR", cache)
        .args([
            "-r",
            repository.to_str().expect("repo"),
            "restore",
            "latest",
            "--target",
        ])
        .arg(target)
        .output()
        .expect("run restic")
        .status
        .success()
}

fn find_named(root: &Path, name: &str) -> PathBuf {
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(&dir).expect("read dir") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                pending.push(path);
            } else if path.file_name().and_then(|n| n.to_str()) == Some(name) {
                return path;
            }
        }
    }
    panic!("{name} not found");
}
