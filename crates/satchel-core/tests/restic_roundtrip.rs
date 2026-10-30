use std::fs;
use std::path::PathBuf;
use std::process::Command;

use satchel_core::{DeviceId, Location, backup_directory, init};
use tempfile::tempdir;

#[test]
fn official_restic_restores_a_snapshot_and_the_key_file_stays_opaque() {
    let workspace = tempdir().expect("temp dir");
    let source = workspace.path().join("source");
    let repository = workspace.path().join("repository");
    let restore = workspace.path().join("restore");
    let cache = workspace.path().join("cache");
    fs::create_dir_all(&source).expect("source");
    fs::create_dir_all(&repository).expect("repository");
    fs::create_dir_all(&cache).expect("cache");
    fs::write(source.join("note.txt"), b"satchel-roundtrip").expect("note");

    let device_id = DeviceId::parse("0123456789abcdef0123456789abcdef").expect("device id");
    let password = "recovery-password";
    let location = Location::Directory(repository.clone());

    init(&location, password, &device_id).expect("init");
    let snapshot_id = backup_directory(&location, password, &device_id, &source).expect("backup");
    assert_eq!(snapshot_id.len(), 64);

    assert_key_file_identity(&repository, device_id.as_str());

    let restic = restic_binary();
    let output = Command::new(&restic)
        .env("RESTIC_PASSWORD", password)
        .env("RESTIC_CACHE_DIR", &cache)
        .args([
            "-r",
            repository.to_str().expect("repo path"),
            "restore",
            "latest",
            "--target",
            restore.to_str().expect("restore path"),
        ])
        .output()
        .expect("run restic");
    assert!(
        output.status.success(),
        "restic restore failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let restored = find_named(&restore, "note.txt");
    let bytes = fs::read(&restored).expect("read restored file");
    assert_eq!(bytes, b"satchel-roundtrip");
}

fn assert_key_file_identity(repository: &std::path::Path, device_id: &str) {
    let keys = repository.join("keys");
    let mut seen = 0_u32;
    for entry in fs::read_dir(&keys).expect("keys dir") {
        let entry = entry.expect("key entry");
        let text = fs::read_to_string(entry.path()).expect("key json");
        let value: serde_json::Value = serde_json::from_str(&text).expect("parse key");
        assert_eq!(value["hostname"].as_str(), Some(device_id));
        assert_eq!(value["username"].as_str(), Some(device_id));
        seen += 1;
    }
    assert_eq!(seen, 1);
}

fn find_named(root: &std::path::Path, name: &str) -> PathBuf {
    let mut found = None;
    for entry in walkdir_files(root) {
        if entry.file_name().and_then(|value| value.to_str()) == Some(name) {
            found = Some(entry);
            break;
        }
    }
    found.expect("restored file")
}

fn walkdir_files(root: &std::path::Path) -> Vec<PathBuf> {
    let mut pending = vec![root.to_path_buf()];
    let mut files = Vec::new();
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(&dir).expect("read dir") {
            let entry = entry.expect("entry");
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else {
                files.push(path);
            }
        }
    }
    files
}

fn restic_binary() -> PathBuf {
    if let Some(path) = std::env::var_os("RESTIC_BIN") {
        return PathBuf::from(path);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.satchel/bin/restic")
}
