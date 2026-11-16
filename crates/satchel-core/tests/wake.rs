//! A device announces where it listens inside its encrypted snapshot, and the
//! other device's sync hands it back as a peer to notify.

use std::fs;
use std::time::Duration;

use satchel_core::{
    DeviceId, Location, WakeEndpoint, WakeListener, add_device_key, generate_wake_key, init,
    send_wakes, sync_folder, sync_folder_announcing,
};
use tempfile::tempdir;

const FIRST: &str = "0123456789abcdef0123456789abcdef";
const SECOND: &str = "fedcba9876543210fedcba9876543210";
const RECOVERY: &str = "recovery-password";
const SECOND_KEY: &str = "second-key-password";

#[test]
fn an_announced_endpoint_reaches_the_other_device_and_a_notice_wakes_the_first() {
    let dir = tempdir().expect("temp");
    let repository = dir.path().join("repository");
    fs::create_dir_all(&repository).expect("repository");
    let location = Location::Directory(repository.clone());
    let first = DeviceId::parse(FIRST).expect("first");
    let second = DeviceId::parse(SECOND).expect("second");
    init(&location, RECOVERY, &first).expect("init");
    add_device_key(&location, RECOVERY, &second, SECOND_KEY).expect("key");
    let first_folder = dir.path().join("first");
    let second_folder = dir.path().join("second");
    fs::create_dir_all(&first_folder).expect("first folder");
    fs::create_dir_all(&second_folder).expect("second folder");
    fs::write(first_folder.join("a.txt"), "alpha").expect("file");

    let key = generate_wake_key();
    let mut listener =
        WakeListener::bind("127.0.0.1:0".parse().expect("addr"), &key).expect("bind");
    let endpoint = WakeEndpoint {
        address: listener.local_addr().expect("addr").to_string(),
        key: key.clone(),
    };
    sync_folder_announcing(
        &location,
        RECOVERY,
        &first,
        &first_folder,
        Some(endpoint.clone()),
    )
    .expect("first sync");

    // The key never sits in the clear in the repository.
    for entry in walk(&repository) {
        let bytes = fs::read(&entry).expect("read");
        assert!(
            !contains(&bytes, key.as_bytes()),
            "wake key in the clear in {}",
            entry.display()
        );
    }

    let pulled = sync_folder(&location, SECOND_KEY, &second, &second_folder).expect("second");
    assert_eq!(pulled.peers.len(), 1);
    assert_eq!(pulled.peers[0].device_id, FIRST);
    assert_eq!(pulled.peers[0].endpoint, endpoint);
    assert_eq!(
        fs::read_to_string(second_folder.join("a.txt")).expect("a"),
        "alpha"
    );

    // The second device wrote a snapshot, so it tells the first to look now.
    fs::write(second_folder.join("b.txt"), "beta").expect("b");
    let wrote = sync_folder(&location, SECOND_KEY, &second, &second_folder).expect("write");
    assert_eq!(send_wakes(&second, &wrote.peers, 10_000), 1);
    assert_eq!(
        listener.wait(Duration::from_secs(2), || 10_000).as_deref(),
        Some(SECOND)
    );

    // A sync without the announcing call keeps the endpoint.
    fs::write(first_folder.join("c.txt"), "gamma").expect("c");
    let kept = sync_folder(&location, RECOVERY, &first, &first_folder).expect("kept");
    assert!(kept.peers.is_empty(), "the second device does not listen");
    let still = sync_folder(&location, SECOND_KEY, &second, &second_folder).expect("still");
    assert_eq!(still.peers.len(), 1);

    // Announcing nothing withdraws it.
    sync_folder_announcing(&location, RECOVERY, &first, &first_folder, None).expect("stop");
    let gone = sync_folder(&location, SECOND_KEY, &second, &second_folder).expect("gone");
    assert!(gone.peers.is_empty());
}

fn walk(root: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut found = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(&dir).expect("dir") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                pending.push(path);
            } else {
                found.push(path);
            }
        }
    }
    found
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}
