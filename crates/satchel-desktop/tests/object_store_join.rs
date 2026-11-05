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
