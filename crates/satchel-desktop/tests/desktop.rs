use std::fs;
use std::net::SocketAddr;
use std::thread;
use std::time::{Duration, Instant};

use satchel_core::{Client, DeviceId, Follow, Location, SetupCheck, init};
use satchel_desktop::{
    Account, Desktop, KeyringStore, MemoryStore, ObjectStore, SecretStore, phone_grant_secret,
};
use satchel_pair::{Error as PairError, JoinRequest, ListenConfig};
use tempfile::tempdir;

fn key_file_names_the_device(repository: &std::path::Path, device_id: &str) -> bool {
    let mut found = false;
    for entry in fs::read_dir(repository.join("keys")).expect("keys") {
        let entry = entry.expect("entry");
        if !entry.file_type().expect("type").is_file() {
            continue;
        }
        let text = fs::read_to_string(entry.path()).expect("key file");
        let value: serde_json::Value = serde_json::from_str(&text).expect("key json");
        if value.get("hostname").and_then(|item| item.as_str()) == Some(device_id) {
            assert_eq!(
                value.get("username").and_then(|item| item.as_str()),
                Some(device_id)
            );
            found = true;
        }
    }
    found
}

fn folder_and_repo(root: &std::path::Path) -> (std::path::PathBuf, std::path::PathBuf) {
    let folder = root.join("folder");
    let repository = root.join("repository");
    fs::create_dir_all(&folder).expect("folder");
    fs::create_dir_all(&repository).expect("repository");
    (folder, repository)
}

#[test]
fn one_folder_is_stored_without_secrets() {
    let root = tempdir().expect("temp");
    let config = root.path().join("config");
    let (folder, repository) = folder_and_repo(root.path());
    let mut desktop = Desktop::open(&config).expect("open");
    let device = DeviceId::generate();
    desktop
        .create_profile(folder.clone(), repository.clone(), device.clone())
        .expect("create");
    let again = desktop.create_profile(folder.clone(), repository.clone(), device.clone());
    assert!(matches!(again, Err(satchel_desktop::Error::ProfileExists)));

    let inside = fresh_create_inside(root.path());
    assert!(matches!(inside, Err(satchel_desktop::Error::Path { .. })));

    let store = MemoryStore::default();
    let recovery = "recovery-canary";
    let bucket = "bucket-fixture";
    store
        .put(Account::RecoveryPassword, recovery)
        .expect("put recovery");
    store
        .put(Account::BucketCredential, bucket)
        .expect("put bucket");
    store
        .put(Account::PhoneCredential, "phone-fixture")
        .expect("put phone");
    assert_eq!(
        store
            .get(Account::RecoveryPassword)
            .expect("get")
            .as_deref(),
        Some(recovery)
    );
    let profile_text = fs::read_to_string(config.join("profile.json")).expect("profile");
    assert!(profile_text.contains(device.as_str()));
    assert!(!profile_text.contains(recovery));
    assert!(!profile_text.contains(bucket));
    let value: serde_json::Value = serde_json::from_str(&profile_text).expect("json");
    let mut keys = Vec::new();
    for key in value.as_object().expect("object").keys() {
        keys.push(key.clone());
    }
    keys.sort();
    assert_eq!(
        keys,
        vec![
            "device_id".to_string(),
            "folder".to_string(),
            "repository".to_string()
        ]
    );
    let loaded = Desktop::open(&config).expect("reload");
    assert_eq!(loaded.profile().expect("profile").folder, folder);
    assert_eq!(loaded.profile().expect("profile").device_id, device);
}

fn fresh_create_inside(root: &std::path::Path) -> Result<(), satchel_desktop::Error> {
    let config = root.join("inside-config");
    let mut desktop = Desktop::open(&config).expect("open");
    let folder = root.join("synced");
    fs::create_dir_all(&folder).expect("folder");
    let repository = folder.join("repository");
    desktop.create_profile(folder, repository, DeviceId::generate())
}

#[test]
fn add_device_screen_approves_without_the_recovery_password() {
    let root = tempdir().expect("temp");
    let config = root.path().join("config");
    let (folder, repository) = folder_and_repo(root.path());
    let mut desktop = Desktop::open(&config).expect("open");
    let holder = DeviceId::parse("0123456789abcdef0123456789abcdef").expect("holder");
    desktop
        .create_profile(folder, repository.clone(), holder.clone())
        .expect("profile");
    init(
        &Location::Directory(repository.clone()),
        "recovery-canary",
        &holder,
    )
    .expect("init");
    let store = MemoryStore::default();
    store
        .put(Account::RecoveryPassword, "recovery-canary")
        .expect("recovery");
    store
        .put(Account::BucketCredential, "maintenance-fixture")
        .expect("bucket");
    store
        .put(Account::PhoneCredential, "bucket-fixture")
        .expect("phone");
    let offer = desktop
        .open_add_device(ListenConfig {
            bind: "127.0.0.1:0".parse().expect("bind"),
            timeout: Duration::from_secs(60),
            advertise: false,
        })
        .expect("screen");
    assert!(desktop.add_device_open());
    let mut saw_qr = false;
    for item in &desktop.tray_view().items {
        if item.label == "QR" {
            let png = item.icon_png.as_ref().expect("qr");
            assert!(png.starts_with(b"\x89PNG"));
            saw_qr = true;
        }
    }
    assert!(saw_qr);
    assert!(desktop.tray_view().invite.is_some());
    let address = offer.address;
    let code = offer.code.clone();
    let fingerprint = offer.fingerprint.clone();
    let joiner = thread::spawn(move || {
        satchel_pair::join(JoinRequest {
            address,
            code,
            fingerprint,
            device_name: "phone".to_string(),
            os: "android".to_string(),
            device_id: "fedcba9876543210fedcba9876543210".to_string(),
            claimed_address: "100.64.0.2:9".to_string(),
            claimed_interface: "tailscale0".to_string(),
            timeout: Duration::from_secs(60),
        })
    });
    let deadline = Instant::now() + Duration::from_secs(60);
    while desktop.pending().is_none() && Instant::now() < deadline {
        desktop.poll();
        if desktop.pending().is_none() {
            thread::sleep(Duration::from_millis(20));
        }
    }
    let pending = desktop.pending().expect("pending");
    assert_eq!(pending.device_name, "phone");
    assert_eq!(pending.os, "android");
    assert!(pending.peer_address.ip().is_loopback());
    assert_eq!(pending.local_interface, "lo");
    let view = desktop.tray_view();
    let mut labels = String::new();
    for item in &view.items {
        labels.push_str(&item.label);
        labels.push('\n');
    }
    assert!(labels.contains("phone"));
    assert!(labels.contains("android"));
    assert!(labels.contains("127.0.0.1"));
    assert!(labels.contains(&offer.code));
    assert!(labels.contains("lo"));
    assert!(!labels.contains("recovery-canary"));
    assert!(!labels.contains("bucket-fixture"));
    desktop.approve_pending(&store).expect("approve");
    let received = joiner.join().expect("thread").expect("grant");
    assert_eq!(received.bucket_credential, "bucket-fixture");
    assert_ne!(received.bucket_credential, "maintenance-fixture");
    assert_eq!(received.endpoint, repository.display().to_string());
    assert_ne!(received.device_key_password, "recovery-canary");
    assert!(!received.device_key_password.is_empty());
    assert!(!format!("{received:?}").contains("recovery-canary"));
    let profile_text = fs::read_to_string(config.join("profile.json")).expect("profile");
    assert!(!profile_text.contains("recovery-canary"));
    assert!(!profile_text.contains("bucket-fixture"));
    assert!(!profile_text.contains("maintenance-fixture"));
    assert!(!profile_text.contains(&received.device_key_password));
    let phone = DeviceId::parse("fedcba9876543210fedcba9876543210").expect("phone");
    let phone_work = root.path().join("phone");
    let phone_client =
        Client::open(phone, &phone_work, &received.device_key_password).expect("phone");
    phone_client
        .catalog(&Location::Directory(repository.clone()))
        .expect("phone catalog");
    assert!(key_file_names_the_device(
        &repository,
        "fedcba9876543210fedcba9876543210"
    ));
    assert!(!desktop.add_device_open());
    let closed = TcpJoin {
        address: offer.address,
    };
    assert!(closed.refused());
}

struct TcpJoin {
    address: SocketAddr,
}

impl TcpJoin {
    fn refused(&self) -> bool {
        std::net::TcpStream::connect_timeout(&self.address, Duration::from_millis(300)).is_err()
    }
}

#[test]
fn closing_the_screen_stops_listening() {
    let root = tempdir().expect("temp");
    let mut desktop = Desktop::open(root.path().join("config")).expect("open");
    let offer = desktop
        .open_add_device(ListenConfig {
            bind: "127.0.0.1:0".parse().expect("bind"),
            timeout: Duration::from_secs(5),
            advertise: false,
        })
        .expect("open screen");
    let port = offer.address.port();
    desktop.close_add_device();
    assert!(!desktop.add_device_open());
    let address = SocketAddr::from(([127, 0, 0, 1], port));
    let connected = std::net::TcpStream::connect_timeout(&address, Duration::from_millis(300));
    assert!(connected.is_err());
}

#[test]
fn deny_from_the_screen_sends_nothing() {
    let root = tempdir().expect("temp");
    let mut desktop = Desktop::open(root.path().join("config")).expect("open");
    let offer = desktop
        .open_add_device(ListenConfig {
            bind: "127.0.0.1:0".parse().expect("bind"),
            timeout: Duration::from_secs(5),
            advertise: false,
        })
        .expect("screen");
    let joiner = thread::spawn(move || {
        satchel_pair::join(JoinRequest {
            address: offer.address,
            code: offer.code,
            fingerprint: offer.fingerprint,
            device_name: "laptop".to_string(),
            os: "linux".to_string(),
            device_id: "fedcba9876543210fedcba9876543210".to_string(),
            claimed_address: "10.1.2.3:9".to_string(),
            claimed_interface: "wg0".to_string(),
            timeout: Duration::from_secs(5),
        })
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    while desktop.pending().is_none() && Instant::now() < deadline {
        desktop.poll();
        thread::sleep(Duration::from_millis(20));
    }
    assert!(desktop.pending().is_some());
    desktop.deny_pending().expect("deny");
    let err = joiner.join().expect("thread").expect_err("no grant");
    assert_eq!(err, PairError::Denied);
}

#[test]
fn approve_without_a_recovery_password_sends_no_grant() {
    let root = tempdir().expect("temp");
    let (folder, repository) = folder_and_repo(root.path());
    let mut desktop = Desktop::open(root.path().join("config")).expect("open");
    desktop
        .create_profile(folder, repository.clone(), DeviceId::generate())
        .expect("profile");
    let store = MemoryStore::default();
    store
        .put(Account::BucketCredential, "bucket-fixture")
        .expect("bucket");
    let offer = desktop
        .open_add_device(ListenConfig {
            bind: "127.0.0.1:0".parse().expect("bind"),
            timeout: Duration::from_secs(5),
            advertise: false,
        })
        .expect("screen");
    let joiner = thread::spawn(move || {
        satchel_pair::join(JoinRequest {
            address: offer.address,
            code: offer.code,
            fingerprint: offer.fingerprint,
            device_name: "phone".to_string(),
            os: "android".to_string(),
            device_id: "fedcba9876543210fedcba9876543210".to_string(),
            claimed_address: "10.9.9.9:9".to_string(),
            claimed_interface: "wg0".to_string(),
            timeout: Duration::from_secs(5),
        })
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    while desktop.pending().is_none() && Instant::now() < deadline {
        desktop.poll();
        thread::sleep(Duration::from_millis(20));
    }
    assert!(desktop.pending().is_some());
    let err = desktop.approve_pending(&store).expect_err("no recovery");
    assert!(matches!(err, satchel_desktop::Error::MissingRecovery));
    assert!(!repository.join("keys").exists());
    desktop.close_add_device();
    let join_err = joiner.join().expect("thread").expect_err("no grant");
    assert_eq!(join_err, PairError::Denied);
}

#[test]
fn os_keystore_roundtrip_uses_a_throwaway_service() {
    let service = format!("satchel-test-{}", DeviceId::generate().as_str());
    let store = KeyringStore::scoped(service);
    let _cleanup = Cleanup(&store);
    store
        .put(Account::RecoveryPassword, "recovery-probe")
        .expect("put recovery");
    store
        .put(Account::BucketCredential, "bucket-probe")
        .expect("put bucket");
    store
        .put(Account::PhoneCredential, "phone-probe")
        .expect("put phone");
    assert_eq!(
        store
            .get(Account::RecoveryPassword)
            .expect("get recovery")
            .as_deref(),
        Some("recovery-probe")
    );
    assert_eq!(
        store
            .get(Account::BucketCredential)
            .expect("get bucket")
            .as_deref(),
        Some("bucket-probe")
    );
    assert_eq!(
        store
            .get(Account::PhoneCredential)
            .expect("get phone")
            .as_deref(),
        Some("phone-probe")
    );
    store.delete(Account::RecoveryPassword).expect("delete");
    assert_eq!(store.get(Account::RecoveryPassword).expect("gone"), None);
}

#[test]
fn approve_without_a_phone_credential_sends_no_grant() {
    let root = tempdir().expect("temp");
    let (folder, repository) = folder_and_repo(root.path());
    let mut desktop = Desktop::open(root.path().join("config")).expect("open");
    desktop
        .create_profile(folder, repository.clone(), DeviceId::generate())
        .expect("profile");
    let store = MemoryStore::default();
    store
        .put(Account::RecoveryPassword, "recovery-canary")
        .expect("recovery");
    store
        .put(Account::BucketCredential, "maintenance-fixture")
        .expect("bucket");
    let offer = desktop
        .open_add_device(ListenConfig {
            bind: "127.0.0.1:0".parse().expect("bind"),
            timeout: Duration::from_secs(5),
            advertise: false,
        })
        .expect("screen");
    let joiner = thread::spawn(move || {
        satchel_pair::join(JoinRequest {
            address: offer.address,
            code: offer.code,
            fingerprint: offer.fingerprint,
            device_name: "phone".to_string(),
            os: "android".to_string(),
            device_id: "fedcba9876543210fedcba9876543210".to_string(),
            claimed_address: "10.9.9.9:9".to_string(),
            claimed_interface: "wg0".to_string(),
            timeout: Duration::from_secs(5),
        })
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    while desktop.pending().is_none() && Instant::now() < deadline {
        desktop.poll();
        thread::sleep(Duration::from_millis(20));
    }
    assert!(desktop.pending().is_some());
    let err = desktop
        .approve_pending(&store)
        .expect_err("no phone credential");
    assert!(matches!(
        err,
        satchel_desktop::Error::MissingPhoneCredential
    ));
    assert!(!repository.join("keys").exists());
    desktop.close_add_device();
    let join_err = joiner.join().expect("thread").expect_err("no grant");
    assert_eq!(join_err, PairError::Denied);
}

#[test]
fn phone_grant_adds_the_bucket_and_keeps_the_desktop_key() {
    let store = ObjectStore {
        bucket: "bucket-a".to_string(),
        endpoint: "http://127.0.0.1:9".to_string(),
        region: "us-east-1".to_string(),
        prefix: "repo".to_string(),
    };
    let opaque = phone_grant_secret(None, "bucket-fixture").expect("opaque");
    assert_eq!(opaque, "bucket-fixture");
    let merged = phone_grant_secret(
        Some(&store),
        r#"{"access_key_id":"phone-key","secret_access_key":"phone-secret"}"#,
    )
    .expect("merged");
    assert!(merged.contains("phone-key"));
    assert!(merged.contains("bucket-a"));
    assert!(merged.contains("repo"));
    assert!(!merged.contains("maintenance-key"));
    let value: serde_json::Value = serde_json::from_str(&merged).expect("json");
    assert_eq!(value["access_key_id"].as_str(), Some("phone-key"));
    assert_eq!(value["bucket"].as_str(), Some("bucket-a"));
}

#[test]
fn publish_lists_metadata_and_revoke_removes_only_the_device_key() {
    let root = tempdir().expect("temp");
    let config = root.path().join("config");
    let (folder, repository) = folder_and_repo(root.path());
    fs::create_dir_all(folder.join("notes")).expect("notes");
    fs::write(folder.join("notes/kept.txt"), "desk-bytes").expect("file");
    let mut desktop = Desktop::open(&config).expect("open");
    let holder = DeviceId::parse("0123456789abcdef0123456789abcdef").expect("holder");
    desktop
        .create_profile(folder, repository.clone(), holder.clone())
        .expect("profile");
    let store = MemoryStore::default();
    store
        .put(Account::RecoveryPassword, "recovery-canary")
        .expect("recovery");
    store
        .put(Account::BucketCredential, "maintenance-fixture")
        .expect("bucket");
    store
        .put(Account::PhoneCredential, "bucket-fixture")
        .expect("phone");
    let report = desktop.publish(&store).expect("publish");
    assert_eq!(report.protection, SetupCheck::Unprotected);
    assert!(report.outcome.cursor.is_some());
    let reader = DeviceId::parse("fedcba9876543210fedcba9876543210").expect("reader");
    let reader_work = root.path().join("reader");
    let reader_client = Client::open(reader, &reader_work, "recovery-canary").expect("reader");
    let catalog = reader_client
        .catalog(&Location::Directory(repository.clone()))
        .expect("catalog");
    let row = catalog
        .iter()
        .find(|entry| entry.path == "notes/kept.txt")
        .expect("row");
    assert_eq!(row.follow, Follow::See);
    assert_eq!(
        row.devices,
        vec!["0123456789abcdef0123456789abcdef".to_string()]
    );
    assert!(!reader_work.join("notes/kept.txt").exists());

    let offer = desktop
        .open_add_device(ListenConfig {
            bind: "127.0.0.1:0".parse().expect("bind"),
            timeout: Duration::from_secs(60),
            advertise: false,
        })
        .expect("screen");
    let address = offer.address;
    let code = offer.code.clone();
    let fingerprint = offer.fingerprint.clone();
    let joiner = thread::spawn(move || {
        satchel_pair::join(JoinRequest {
            address,
            code,
            fingerprint,
            device_name: "phone".to_string(),
            os: "android".to_string(),
            device_id: "fedcba9876543210fedcba9876543210".to_string(),
            claimed_address: "10.1.2.3:9".to_string(),
            claimed_interface: "wg0".to_string(),
            timeout: Duration::from_secs(60),
        })
    });
    let deadline = Instant::now() + Duration::from_secs(60);
    while desktop.pending().is_none() && Instant::now() < deadline {
        desktop.poll();
        thread::sleep(Duration::from_millis(20));
    }
    desktop.approve_pending(&store).expect("approve");
    let received = joiner.join().expect("thread").expect("grant");
    desktop.load_device_keys(&store).expect("keys");
    let mut labels = String::new();
    for item in &desktop.tray_view().items {
        labels.push_str(&item.label);
        labels.push('\n');
    }
    assert!(labels.contains("Revoke fedcba9876543210fedcba9876543210"));
    assert!(!labels.contains("recovery-canary"));
    let mut key_id = String::new();
    for item in desktop.tray_view().items {
        if let satchel_desktop::TrayCommand::Revoke(id) = item.command {
            key_id = id;
        }
    }
    assert_eq!(key_id.len(), 64);
    desktop.revoke_device_key(&store, &key_id).expect("revoke");
    let phone = DeviceId::parse("fedcba9876543210fedcba9876543210").expect("phone");
    let revoked = Client::open(
        phone,
        root.path().join("revoked"),
        &received.device_key_password,
    )
    .expect("open does not contact the repository");
    assert!(
        revoked
            .catalog(&Location::Directory(repository.clone()))
            .is_err()
    );
    let still =
        Client::open(holder, root.path().join("still"), "recovery-canary").expect("recovery");
    still
        .catalog(&Location::Directory(repository))
        .expect("recovery catalog");
}

struct Cleanup<'a>(&'a KeyringStore);

impl Drop for Cleanup<'_> {
    fn drop(&mut self) {
        let _ = self.0.delete(Account::RecoveryPassword);
        let _ = self.0.delete(Account::BucketCredential);
        let _ = self.0.delete(Account::PhoneCredential);
    }
}
