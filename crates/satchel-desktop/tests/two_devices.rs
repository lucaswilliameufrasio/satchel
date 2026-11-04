//! Either device can create the repository. The other joins by code and
//! never types the storage account again.

use std::fs;
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

use satchel_core::{StorageKind, StorageSetup};
use satchel_desktop::{
    Account, Desktop, JoinTarget, MemoryStore, Role, SecretStore, request_grant,
};
use satchel_pair::ListenConfig;
use tempfile::tempdir;

fn listen() -> ListenConfig {
    ListenConfig {
        bind: "127.0.0.1:0".parse().expect("bind"),
        timeout: Duration::from_secs(60),
        advertise: false,
    }
}

fn key_file_texts(repository: &Path) -> Vec<String> {
    let mut texts = Vec::new();
    for entry in fs::read_dir(repository.join("keys")).expect("keys") {
        let entry = entry.expect("entry");
        texts.push(fs::read_to_string(entry.path()).expect("key"));
    }
    texts
}

/// Run one pairing: `holder` approves while a thread joins with the invite.
fn pair(
    holder: &mut Desktop,
    holder_store: &MemoryStore,
    joiner: &mut Desktop,
    joiner_store: &MemoryStore,
    joiner_folder: std::path::PathBuf,
    approve: bool,
) -> Result<(), satchel_desktop::Error> {
    let offer = holder.open_add_device(listen()).expect("screen");
    let target = JoinTarget::from_invite(&offer.invite, "second", "test").expect("invite");
    let grant = thread::spawn(move || request_grant(target, Duration::from_secs(60)));
    let deadline = Instant::now() + Duration::from_secs(60);
    while holder.pending().is_none() && Instant::now() < deadline {
        holder.poll();
        thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(holder.pending().expect("pending").device_name, "second");
    if approve {
        holder.approve_pending(holder_store).expect("approve");
    } else {
        holder.deny_pending().expect("deny");
    }
    let granted = grant.join().expect("thread")?;
    joiner.join_with_grant(joiner_store, joiner_folder, granted)
}

#[test]
fn folder_repository_is_created_by_one_device_and_joined_by_the_other_without_the_recovery_password()
 {
    let root = tempdir().expect("temp");
    let repository = root.path().join("repo");
    let first_folder = root.path().join("first");
    let second_folder = root.path().join("second");
    fs::create_dir_all(&first_folder).expect("first");
    fs::create_dir_all(&second_folder).expect("second");
    fs::write(first_folder.join("note.txt"), "from-first").expect("note");

    let first_store = MemoryStore::default();
    let mut first = Desktop::open(root.path().join("config-first")).expect("open");
    let created = first
        .create_repository(
            &first_store,
            first_folder.clone(),
            StorageSetup::directory(&repository),
            None,
        )
        .expect("create");
    assert_eq!(created.recovery_password.len(), 64);
    assert_eq!(
        first_store.get(Account::RecoveryPassword).expect("get"),
        Some(created.recovery_password.clone())
    );
    let profile = first.profile().expect("profile");
    assert_eq!(profile.role, Role::Maintainer);
    assert_eq!(profile.kind, StorageKind::Directory);
    let second_create = first.create_repository(
        &first_store,
        first_folder.clone(),
        StorageSetup::directory(root.path().join("other")),
        None,
    );
    assert!(second_create.is_err());

    first.publish(&first_store).expect("first publish");

    let second_store = MemoryStore::default();
    let mut second = Desktop::open(root.path().join("config-second")).expect("open");
    pair(
        &mut first,
        &first_store,
        &mut second,
        &second_store,
        second_folder.clone(),
        true,
    )
    .expect("join");

    assert_eq!(second.profile().expect("profile").role, Role::Member);
    assert!(
        second_store
            .get(Account::RecoveryPassword)
            .expect("get")
            .is_none()
    );
    assert!(
        second_store
            .get(Account::DeviceKeyPassword)
            .expect("get")
            .is_some()
    );
    let second_view = second.tray_view();
    let labels: Vec<&str> = second_view.items.iter().map(|i| i.label.as_str()).collect();
    assert!(labels.contains(&"Sync"));
    assert!(!labels.contains(&"Add device"));
    assert!(labels.iter().any(|l| l.starts_with("Storage folder")));

    second.publish(&second_store).expect("second sync");
    assert_eq!(
        fs::read_to_string(second_folder.join("note.txt")).expect("pulled"),
        "from-first"
    );
    fs::write(second_folder.join("reply.txt"), "from-second").expect("reply");
    second.publish(&second_store).expect("second sends");
    first.publish(&first_store).expect("first pulls");
    assert_eq!(
        fs::read_to_string(first_folder.join("reply.txt")).expect("reply back"),
        "from-second"
    );

    // The timer skips a cycle with nothing new, and wakes for either side.
    assert!(first.auto_sync(&first_store).expect("idle").is_none());
    assert!(
        second.auto_sync(&second_store).expect("idle").is_some(),
        "first's pull is news"
    );
    assert!(second.auto_sync(&second_store).expect("idle").is_none());
    thread::sleep(Duration::from_millis(20));
    fs::write(first_folder.join("late.txt"), "late").expect("late");
    assert!(first.auto_sync(&first_store).expect("local edit").is_some());
    assert!(
        second
            .auto_sync(&second_store)
            .expect("remote news")
            .is_some()
    );
    assert_eq!(
        fs::read_to_string(second_folder.join("late.txt")).expect("late pulled"),
        "late"
    );
    // Each side may write one snapshot that wakes the other once. They must settle.
    let mut rounds = 0;
    loop {
        let woke_first = first.auto_sync(&first_store).expect("settle").is_some();
        let woke_second = second.auto_sync(&second_store).expect("settle").is_some();
        eprintln!("settle round {rounds}: first {woke_first} second {woke_second}");
        if !woke_first && !woke_second {
            break;
        }
        rounds += 1;
        assert!(rounds < 4, "the two devices keep waking each other");
    }

    // Two keys, both opaque. The member never had the recovery password.
    let keys = key_file_texts(&repository);
    assert_eq!(keys.len(), 2);
    for text in &keys {
        assert!(!text.contains("created"));
    }
    assert!(second.approve_pending(&second_store).is_err());
    assert!(second.load_device_keys(&second_store).is_err());
}

#[test]
fn denied_pairing_leaves_no_profile_and_no_key() {
    let root = tempdir().expect("temp");
    let repository = root.path().join("repo");
    let first_folder = root.path().join("first");
    let second_folder = root.path().join("second");
    fs::create_dir_all(&first_folder).expect("first");
    fs::create_dir_all(&second_folder).expect("second");
    let first_store = MemoryStore::default();
    let mut first = Desktop::open(root.path().join("config-first")).expect("open");
    first
        .create_repository(
            &first_store,
            first_folder,
            StorageSetup::directory(&repository),
            None,
        )
        .expect("create");
    let second_store = MemoryStore::default();
    let mut second = Desktop::open(root.path().join("config-second")).expect("open");
    let outcome = pair(
        &mut first,
        &first_store,
        &mut second,
        &second_store,
        second_folder,
        false,
    );
    assert!(outcome.is_err());
    assert!(second.profile().is_none());
    assert!(
        second_store
            .get(Account::DeviceKeyPassword)
            .expect("get")
            .is_none()
    );
    assert_eq!(key_file_texts(&repository).len(), 1);
}

#[test]
fn object_storage_needs_the_limited_credential_and_a_different_key() {
    let root = tempdir().expect("temp");
    let folder = root.path().join("folder");
    fs::create_dir_all(&folder).expect("folder");
    let mut setup = StorageSetup::directory(root.path().join("unused"));
    setup.kind = StorageKind::B2;
    setup.endpoint = "http://127.0.0.1:9".to_string();
    setup.bucket = "bucket".to_string();
    setup.region = "us-west-004".to_string();
    setup.access_key_id = "maintenance".to_string();
    setup.secret_access_key = "maintenance-secret".to_string();
    let store = MemoryStore::default();
    let mut desktop = Desktop::open(root.path().join("config")).expect("open");
    assert!(
        desktop
            .create_repository(&store, folder.clone(), setup.clone(), None)
            .is_err()
    );
    let same = satchel_desktop::AccessPair {
        access_key_id: "maintenance".to_string(),
        secret_access_key: "x".to_string(),
    };
    assert!(
        desktop
            .create_repository(&store, folder, setup, Some(same))
            .is_err()
    );
    assert!(desktop.profile().is_none());
    assert!(store.get(Account::RecoveryPassword).expect("get").is_none());
}
