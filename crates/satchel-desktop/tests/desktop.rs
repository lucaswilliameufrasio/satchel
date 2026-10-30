use std::fs;
use std::net::SocketAddr;
use std::thread;
use std::time::{Duration, Instant};

use satchel_core::DeviceId;
use satchel_desktop::{Account, Desktop, KeyringStore, MemoryStore, SecretStore};
use satchel_pair::{Error as PairError, JoinRequest, ListenConfig};
use tempfile::tempdir;

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
    desktop
        .create_profile(folder, repository.clone(), DeviceId::generate())
        .expect("profile");
    let store = MemoryStore::default();
    store
        .put(Account::RecoveryPassword, "recovery-canary")
        .expect("recovery");
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
    assert!(desktop.add_device_open());
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
            claimed_address: "100.64.0.2:9".to_string(),
            claimed_interface: "tailscale0".to_string(),
            timeout: Duration::from_secs(5),
        })
    });
    let deadline = Instant::now() + Duration::from_secs(5);
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
    assert_eq!(received.endpoint, repository.display().to_string());
    assert_ne!(received.device_key_password, "recovery-canary");
    assert!(!received.device_key_password.is_empty());
    assert!(!format!("{received:?}").contains("recovery-canary"));
    let profile_text = fs::read_to_string(config.join("profile.json")).expect("profile");
    assert!(!profile_text.contains("recovery-canary"));
    assert!(!profile_text.contains("bucket-fixture"));
    assert!(!profile_text.contains(&received.device_key_password));
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
    store.delete(Account::RecoveryPassword).expect("delete");
    assert_eq!(store.get(Account::RecoveryPassword).expect("gone"), None);
}

struct Cleanup<'a>(&'a KeyringStore);

impl Drop for Cleanup<'_> {
    fn drop(&mut self) {
        let _ = self.0.delete(Account::RecoveryPassword);
        let _ = self.0.delete(Account::BucketCredential);
    }
}
