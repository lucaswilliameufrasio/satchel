//! A second desktop joins a bucket repository without typing the storage account,
//! and both keep the same folder. Floci stands in for the bucket.

#[path = "../../satchel-core/tests/support/floci.rs"]
mod floci;

use std::fs;
use std::thread;
use std::time::{Duration, Instant};

use floci::{ACCESS_KEY, LocalFloci, SECRET_KEY, TempRoot, aws};
use satchel_core::{StorageKind, StorageSetup};
use satchel_desktop::{
    AccessPair, Account, Desktop, JoinTarget, Language, MemoryStore, Role, SecretStore,
    request_grant,
};
use satchel_pair::ListenConfig;

#[test]
fn second_desktop_joins_a_bucket_and_both_sync() {
    let floci = LocalFloci::start();
    let endpoint = floci.endpoint().to_string();
    let bucket = format!("satchel-join-{}", std::process::id());
    aws(
        &endpoint,
        &["s3api", "create-bucket", "--bucket", bucket.as_str()],
    );
    aws(
        &endpoint,
        &[
            "s3api",
            "put-bucket-versioning",
            "--bucket",
            bucket.as_str(),
            "--versioning-configuration",
            "Status=Enabled",
        ],
    );
    let root = TempRoot::new();
    let first_folder = root.0.join("first");
    let second_folder = root.0.join("second");
    fs::create_dir_all(&first_folder).expect("first");
    fs::create_dir_all(&second_folder).expect("second");
    fs::write(first_folder.join("note.txt"), "from-first").expect("note");

    let mut setup = StorageSetup::directory(root.0.join("unused"));
    setup.kind = StorageKind::R2;
    setup.endpoint = endpoint.clone();
    setup.bucket = bucket.clone();
    setup.region = "us-east-1".to_string();
    setup.prefix = "repo".to_string();
    setup.access_key_id = ACCESS_KEY.to_string();
    setup.secret_access_key = SECRET_KEY.to_string();

    let first_store = MemoryStore::default();
    let mut first = Desktop::open(root.0.join("config-first"))
        .expect("open")
        .with_language(Language::En);
    let created = first
        .create_repository(
            &first_store,
            first_folder.clone(),
            setup,
            Some(AccessPair {
                access_key_id: "device-key".to_string(),
                secret_access_key: "device-secret".to_string(),
            }),
        )
        .expect("create on the bucket");
    assert_eq!(created.protection, satchel_core::SetupCheck::Ready);
    assert_eq!(first.profile().expect("profile").kind, StorageKind::R2);
    first.publish(&first_store).expect("first publishes");

    // The invite never names the bucket; the grant does, after approval.
    let offer = first
        .open_add_device(ListenConfig {
            bind: "127.0.0.1:0".parse().expect("bind"),
            timeout: Duration::from_secs(90),
            advertise: false,
        })
        .expect("screen");
    assert!(!offer.invite.contains(&bucket));
    let target = JoinTarget::from_invite(&offer.invite, "second", "test").expect("invite");
    let grant = thread::spawn(move || request_grant(target, Duration::from_secs(90)));
    let deadline = Instant::now() + Duration::from_secs(60);
    while first.pending().is_none() {
        first.poll();
        assert!(Instant::now() < deadline, "no approval request");
        thread::sleep(Duration::from_millis(30));
    }
    first.approve_pending(&first_store).expect("approve");
    let granted = grant.join().expect("thread").expect("grant");

    let second_store = MemoryStore::default();
    let mut second = Desktop::open(root.0.join("config-second"))
        .expect("open")
        .with_language(Language::En);
    second
        .join_with_grant(&second_store, second_folder.clone(), granted)
        .expect("join the bucket");
    let profile = second.profile().expect("profile");
    assert_eq!(profile.role, Role::Member);
    let object = profile.object_store.as_ref().expect("bucket coordinates");
    assert_eq!(object.bucket, bucket);
    assert_eq!(object.prefix, "repo");
    // The member holds the limited key, never the maintenance key or the recovery password.
    let stored = second_store
        .get(Account::BucketCredential)
        .expect("get")
        .expect("credential");
    assert!(stored.contains("device-key"));
    assert!(!stored.contains(ACCESS_KEY));
    assert!(
        second_store
            .get(Account::RecoveryPassword)
            .expect("get")
            .is_none()
    );

    second.publish(&second_store).expect("second syncs");
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

    // The official restic still opens it with the recovery password.
    let recovery = first_store
        .get(Account::RecoveryPassword)
        .expect("get")
        .expect("recovery");
    assert_eq!(recovery, created.recovery_password);
    let restored = root.0.join("restore");
    restic_restore(&endpoint, &bucket, &root.0, &restored, &recovery);
    assert_eq!(
        fs::read(restored.join("satchel/reply.txt")).expect("restored"),
        b"from-second"
    );

    // Each device says who it is, and the one that joined knows which is main.
    let main_name = first.profile().expect("profile").name.clone();
    assert_eq!(second.profile().expect("profile").name, "second");
    second
        .publish(&second_store)
        .expect("second reads the devices");
    assert_eq!(
        second.main_device_name().as_deref(),
        Some(main_name.as_str())
    );
    let view = second.tray_view();
    assert!(
        view.items
            .iter()
            .any(|item| item.label == format!("Main device: {main_name}")),
        "{:?}",
        view.items
            .iter()
            .map(|item| &item.label)
            .collect::<Vec<_>>()
    );
    let add = view
        .items
        .iter()
        .find(|item| item.label == "Add device…")
        .expect("a member adds devices too");
    assert!(add.enabled);

    // The device that joined adds a third one, without the main device.
    let offer = second
        .open_add_device(ListenConfig {
            bind: "127.0.0.1:0".parse().expect("bind"),
            timeout: Duration::from_secs(90),
            advertise: false,
        })
        .expect("screen on the member");
    let target = JoinTarget::from_invite(&offer.invite, "third", "test").expect("invite");
    let grant = thread::spawn(move || request_grant(target, Duration::from_secs(90)));
    let deadline = Instant::now() + Duration::from_secs(60);
    while second.pending().is_none() {
        second.poll();
        assert!(Instant::now() < deadline, "no approval request");
        thread::sleep(Duration::from_millis(30));
    }
    second
        .approve_pending(&second_store)
        .expect("member approves");
    let granted = grant.join().expect("thread").expect("grant");
    let third_folder = root.0.join("third");
    fs::create_dir_all(&third_folder).expect("third");
    let third_store = MemoryStore::default();
    let mut third = Desktop::open(root.0.join("config-third"))
        .expect("open")
        .with_language(Language::En);
    third
        .join_with_grant(&third_store, third_folder.clone(), granted)
        .expect("third joins through the member");
    third.publish(&third_store).expect("third syncs");
    assert_eq!(
        fs::read_to_string(third_folder.join("note.txt")).expect("pulled"),
        "from-first"
    );
    // What the member sent is the limited key, never more.
    let sent = third_store
        .get(Account::BucketCredential)
        .expect("get")
        .expect("credential");
    assert!(sent.contains("device-key"));
    assert!(!sent.contains(ACCESS_KEY));

    // With the main device gone, the member becomes main: both secrets are checked first.
    let wrong = second.become_main(
        &second_store,
        "not-the-recovery-password",
        Some(AccessPair {
            access_key_id: ACCESS_KEY.to_string(),
            secret_access_key: SECRET_KEY.to_string(),
        }),
    );
    assert!(
        matches!(wrong, Err(satchel_desktop::Error::NotTheRecovery)),
        "{wrong:?}"
    );
    assert_eq!(second.profile().expect("profile").role, Role::Member);
    second
        .become_main(
            &second_store,
            &recovery,
            Some(AccessPair {
                access_key_id: ACCESS_KEY.to_string(),
                secret_access_key: SECRET_KEY.to_string(),
            }),
        )
        .expect("become main");
    assert_eq!(second.profile().expect("profile").role, Role::Maintainer);
    second
        .load_device_keys(&second_store)
        .expect("now lists devices to remove");
    second.publish(&second_store).expect("syncs as main");
    let reopened = Desktop::open(root.0.join("config-second")).expect("reopen");
    assert_eq!(reopened.profile().expect("profile").role, Role::Maintainer);
}

#[test]
fn the_device_key_must_differ_from_the_maintenance_key() {
    let root = TempRoot::new();
    let folder = root.0.join("folder");
    fs::create_dir_all(&folder).expect("folder");
    let mut setup = StorageSetup::directory(root.0.join("unused"));
    setup.kind = StorageKind::R2;
    setup.endpoint = "https://example.invalid".to_string();
    setup.bucket = "bucket".to_string();
    setup.region = "auto".to_string();
    setup.access_key_id = ACCESS_KEY.to_string();
    setup.secret_access_key = SECRET_KEY.to_string();
    let store = MemoryStore::default();
    let mut desktop = Desktop::open(root.0.join("config")).expect("open");
    let same = desktop.create_repository(
        &store,
        folder.clone(),
        setup.clone(),
        Some(AccessPair {
            access_key_id: ACCESS_KEY.to_string(),
            secret_access_key: "other".to_string(),
        }),
    );
    assert!(matches!(
        same,
        Err(satchel_desktop::Error::DeviceKeySameAsMaintenance)
    ));
    let empty = desktop.create_repository(&store, folder, setup, None);
    assert!(matches!(
        empty,
        Err(satchel_desktop::Error::DeviceKeyMissing)
    ));
    assert!(
        store.get(Account::RecoveryPassword).expect("get").is_none(),
        "nothing kept"
    );
}

fn restic_restore(
    endpoint: &str,
    bucket: &str,
    root: &std::path::Path,
    restored: &std::path::Path,
    password: &str,
) {
    let binary = match std::env::var_os("RESTIC_BIN") {
        Some(path) => std::path::PathBuf::from(path),
        None => {
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.satchel/bin/restic")
        }
    };
    let cache = root.join("cache");
    fs::create_dir_all(&cache).expect("cache");
    let repository = format!("s3:{endpoint}/{bucket}/repo");
    let output = std::process::Command::new(binary)
        .env("RESTIC_PASSWORD", password)
        .env("RESTIC_CACHE_DIR", &cache)
        .env("AWS_ACCESS_KEY_ID", ACCESS_KEY)
        .env("AWS_SECRET_ACCESS_KEY", SECRET_KEY)
        .env("AWS_DEFAULT_REGION", "us-east-1")
        .env("AWS_CONFIG_FILE", "/dev/null")
        .env("AWS_SHARED_CREDENTIALS_FILE", "/dev/null")
        .args([
            "-o",
            "s3.bucket-lookup=path",
            "-r",
            repository.as_str(),
            "restore",
            "latest",
            "--target",
        ])
        .arg(restored)
        .output()
        .expect("restic");
    assert!(output.status.success(), "restic status {}", output.status);
}
