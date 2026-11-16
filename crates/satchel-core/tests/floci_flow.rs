//! Desktop and phone share one repository on a local S3 emulator.
//!
//! `cargo test` starts `floci/floci:2.3.0` through testcontainers, publishes
//! port 4566 on `127.0.0.1` only, and removes that container on teardown.
//! The image must already be local. This test does not read `FLOCI_ENDPOINT`.
//! A rootless daemon is used when that context is a local socket and reports
//! `name=rootless`. Otherwise the current context is used, and only when its
//! endpoint is a local `unix://` socket.

use std::fs;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use satchel_core::{
    Client, DeviceId, Follow, Location, S3Store, SetupCheck, StorageKind, StorageSetup,
    add_device_key, check_setup, create_repository, probe_object_protection, publish_folder,
    read_version, sync_folder,
};
#[path = "support/floci.rs"]
mod floci;
use floci::*;

const DESKTOP: &str = "0123456789abcdef0123456789abcdef";
const PHONE: &str = "fedcba9876543210fedcba9876543210";
const PASSWORD: &str = "recovery-password";
const DEVICE_PASSWORD: &str = "device-key-password";

#[test]
fn desktop_publishes_and_the_phone_reads_metadata_only() {
    let floci = LocalFloci::start();
    let endpoint = floci.endpoint();
    let bucket = format!("satchel-flow-{}", std::process::id());
    aws(
        endpoint,
        &["s3api", "create-bucket", "--bucket", bucket.as_str()],
    );
    aws(
        endpoint,
        &[
            "s3api",
            "put-bucket-versioning",
            "--bucket",
            bucket.as_str(),
            "--versioning-configuration",
            "Status=Enabled",
        ],
    );
    let probed = probe_object_protection(endpoint, &bucket, "us-east-1", ACCESS_KEY, SECRET_KEY)
        .expect("probe");
    assert_eq!(probed.versioning, Some(satchel_core::Versioning::Enabled));
    assert!(!probed.lifecycle_deletes_objects);
    let gate = check_setup(
        StorageKind::S3,
        probed.versioning,
        probed.lifecycle_deletes_objects,
    )
    .expect("gate");
    assert_eq!(gate, SetupCheck::Ready);

    let location = s3_location(&bucket, endpoint);
    let root = TempRoot::new();
    let source = root.0.join("desktop");
    fs::create_dir_all(source.join("notes")).expect("source");
    fs::write(source.join("notes/from-desktop.txt"), "desk-bytes").expect("file");
    let desktop_id = DeviceId::parse(DESKTOP).expect("desktop id");
    publish_folder(&location, PASSWORD, &desktop_id, &source).expect("publish");
    let phone_id = DeviceId::parse(PHONE).expect("phone id");
    add_device_key(&location, PASSWORD, &phone_id, DEVICE_PASSWORD).expect("device key");

    let phone_work = root.0.join("phone");
    let phone = Client::open(phone_id, &phone_work, DEVICE_PASSWORD).expect("phone");
    let catalog = phone.catalog(&location).expect("catalog");
    let row = catalog
        .iter()
        .find(|entry| entry.path == "notes/from-desktop.txt")
        .expect("metadata");
    assert_eq!(row.follow, Follow::See);
    assert_eq!(row.devices, vec![DESKTOP.to_string()]);
    assert!(
        !phone_work.join("notes/from-desktop.txt").exists(),
        "first look wrote the work copy"
    );
    let bytes = read_version(
        &location,
        DEVICE_PASSWORD,
        &row_snapshot(&location),
        "notes/from-desktop.txt",
    )
    .expect("read");
    assert_eq!(bytes, b"desk-bytes");
    assert!(!phone_work.join("notes/from-desktop.txt").exists());

    assert_key_names(endpoint, &bucket, &root.0);
    let restored = root.0.join("restic-restore");
    restore_with_official_restic(endpoint, &bucket, &root.0.join("restic-cache"), &restored);
    let restored_bytes =
        fs::read(restored.join("satchel/notes/from-desktop.txt")).expect("restored");
    assert_eq!(restored_bytes, b"desk-bytes");

    if let Ok(path) = std::env::var("FLOCI_PHONE_SPEC")
        && !path.is_empty()
    {
        write_phone_spec(Path::new(&path), &bucket, endpoint);
    }
}

fn object_setup(kind: StorageKind, bucket: &str, endpoint: &str) -> StorageSetup {
    let mut setup = StorageSetup::directory(PathBuf::new());
    setup.kind = kind;
    setup.endpoint = endpoint.to_string();
    setup.bucket = bucket.to_string();
    setup.region = "us-east-1".to_string();
    setup.prefix = "repo".to_string();
    setup.access_key_id = ACCESS_KEY.to_string();
    setup.secret_access_key = SECRET_KEY.to_string();
    setup
}

fn config_object_exists(endpoint: &str, bucket: &str) -> bool {
    aws_output(
        endpoint,
        &[
            "s3api",
            "head-object",
            "--bucket",
            bucket,
            "--key",
            "repo/config",
        ],
    )
    .status
    .success()
}

#[test]
fn setup_refuses_unprotected_buckets_before_writing_and_accepts_a_protected_one() {
    let floci = LocalFloci::start();
    let endpoint = floci.endpoint();
    let bucket = format!("satchel-setup-{}", std::process::id());
    aws(
        endpoint,
        &["s3api", "create-bucket", "--bucket", bucket.as_str()],
    );
    let device = DeviceId::parse(DESKTOP).expect("device");

    // Versioning never turned on. R2 has no versioning, so it is tested on its own bucket below.
    for kind in [StorageKind::S3, StorageKind::B2] {
        let setup = object_setup(kind, &bucket, endpoint);
        let refused = create_repository(&setup, &device, PASSWORD);
        assert!(
            matches!(refused, Err(satchel_core::Error::ObjectProtection)),
            "{kind:?} was not refused"
        );
        assert!(!config_object_exists(endpoint, &bucket));
    }

    // Versioning suspended.
    aws(
        endpoint,
        &[
            "s3api",
            "put-bucket-versioning",
            "--bucket",
            bucket.as_str(),
            "--versioning-configuration",
            "Status=Enabled",
        ],
    );
    aws(
        endpoint,
        &[
            "s3api",
            "put-bucket-versioning",
            "--bucket",
            bucket.as_str(),
            "--versioning-configuration",
            "Status=Suspended",
        ],
    );
    let setup = object_setup(StorageKind::S3, &bucket, endpoint);
    assert!(matches!(
        create_repository(&setup, &device, PASSWORD),
        Err(satchel_core::Error::ObjectProtection)
    ));
    assert!(!config_object_exists(endpoint, &bucket));

    // Versioning on, lifecycle that expires objects.
    aws(
        endpoint,
        &[
            "s3api",
            "put-bucket-versioning",
            "--bucket",
            bucket.as_str(),
            "--versioning-configuration",
            "Status=Enabled",
        ],
    );
    aws(
        endpoint,
        &[
            "s3api",
            "put-bucket-lifecycle-configuration",
            "--bucket",
            bucket.as_str(),
            "--lifecycle-configuration",
            "{\"Rules\":[{\"ID\":\"expire\",\"Status\":\"Enabled\",\"Filter\":{\"Prefix\":\"\"},\"Expiration\":{\"Days\":1}}]}",
        ],
    );
    assert!(matches!(
        create_repository(&setup, &device, PASSWORD),
        Err(satchel_core::Error::ObjectProtection)
    ));
    // R2 too: a rule that deletes objects would eat the repository.
    assert!(matches!(
        create_repository(
            &object_setup(StorageKind::R2, &bucket, endpoint),
            &device,
            PASSWORD
        ),
        Err(satchel_core::Error::ObjectProtection)
    ));
    assert!(!config_object_exists(endpoint, &bucket));
    aws(
        endpoint,
        &[
            "s3api",
            "delete-bucket-lifecycle",
            "--bucket",
            bucket.as_str(),
        ],
    );

    // Protected: created once, and the official restic opens it.
    let made = create_repository(&setup, &device, PASSWORD).expect("create");
    assert_eq!(made.protection, SetupCheck::Ready);
    assert!(config_object_exists(endpoint, &bucket));
    let again = create_repository(&setup, &device, PASSWORD);
    assert!(
        matches!(again, Err(satchel_core::Error::RepositoryExists)),
        "second create: {again:?}"
    );
    let root = TempRoot::new();
    let folder = root.0.join("folder");
    fs::create_dir_all(&folder).expect("folder");
    fs::write(folder.join("made.txt"), "made-bytes").expect("file");
    let location = setup.location().expect("location");
    sync_folder(&location, PASSWORD, &device, &folder).expect("sync");
    let restored = root.0.join("restore");
    restore_with_official_restic(endpoint, &bucket, &root.0.join("cache"), &restored);
    assert_eq!(
        fs::read(restored.join("satchel/made.txt")).expect("restored"),
        b"made-bytes"
    );
}

#[test]
fn r2_has_no_versioning_so_it_is_created_with_a_warning() {
    let floci = LocalFloci::start();
    let endpoint = floci.endpoint();
    let bucket = format!("satchel-r2-{}", std::process::id());
    aws(
        endpoint,
        &["s3api", "create-bucket", "--bucket", bucket.as_str()],
    );
    let device = DeviceId::parse(DESKTOP).expect("device");
    let setup = object_setup(StorageKind::R2, &bucket, endpoint);
    let made = create_repository(&setup, &device, PASSWORD).expect("R2 without versioning");
    assert_eq!(made.protection, SetupCheck::Unprotected);
    assert!(config_object_exists(endpoint, &bucket));
}

fn row_snapshot(location: &Location) -> String {
    let listed = satchel_core::cursors(location, DEVICE_PASSWORD).expect("cursors");
    let mut chosen: Option<&satchel_core::Cursor> = None;
    for cursor in &listed {
        let replace = match chosen {
            None => true,
            Some(current) => cursor.time_unix > current.time_unix,
        };
        if replace {
            chosen = Some(cursor);
        }
    }
    chosen.expect("snapshot").snapshot_id.clone()
}

fn s3_location(bucket: &str, endpoint: &str) -> Location {
    Location::S3(
        S3Store::new(
            bucket,
            endpoint,
            "us-east-1",
            "repo",
            ACCESS_KEY,
            SECRET_KEY,
        )
        .expect("store"),
    )
}

fn write_phone_spec(path: &Path, bucket: &str, endpoint: &str) {
    let phone_endpoint = endpoint.replace("127.0.0.1", "10.0.2.2");
    let body = serde_json::json!({
        "bucket": bucket,
        "endpoint": phone_endpoint,
        "region": "us-east-1",
        "prefix": "repo",
        "access_key_id": ACCESS_KEY,
        "secret_access_key": SECRET_KEY,
        "password": DEVICE_PASSWORD,
        "work": "/data/user/0/io.github.lucaswilliameufrasio.satchel/cache/shared-work",
    });
    let text = serde_json::to_string(&body).expect("spec");
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("spec dir");
    }
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
        .expect("spec file");
    std::io::Write::write_all(&mut file, text.as_bytes()).expect("spec write");
}

fn assert_key_names(endpoint: &str, bucket: &str, root: &Path) {
    let listed = aws_stdout(
        endpoint,
        &[
            "s3api",
            "list-objects-v2",
            "--bucket",
            bucket,
            "--prefix",
            "repo/keys/",
            "--query",
            "Contents[].Key",
            "--output",
            "text",
        ],
    );
    let prefix = "repo/keys/";
    let mut names = Vec::new();
    let mut index = 0;
    for part in listed.split_whitespace() {
        if !part.starts_with(prefix) || part.len() <= prefix.len() {
            continue;
        }
        let dest = root.join(format!("key-{index}.json"));
        index += 1;
        aws_to_file(
            endpoint,
            &["s3api", "get-object", "--bucket", bucket, "--key", part],
            &dest,
        );
        let text = fs::read_to_string(&dest).expect("key json");
        let value: serde_json::Value = serde_json::from_str(&text).expect("parse key");
        let hostname = value["hostname"].as_str().expect("hostname").to_string();
        assert_eq!(value["username"].as_str(), Some(hostname.as_str()));
        names.push(hostname);
    }
    names.sort();
    assert_eq!(names, vec![DESKTOP.to_string(), PHONE.to_string()]);
}

fn restore_with_official_restic(endpoint: &str, bucket: &str, cache: &Path, restore: &Path) {
    fs::create_dir_all(cache).expect("restic cache");
    let repository = format!("s3:{endpoint}/{bucket}/repo");
    let mut command = Command::new(restic_binary());
    command.env("RESTIC_PASSWORD", PASSWORD);
    command.env("RESTIC_CACHE_DIR", cache);
    command.env("AWS_ACCESS_KEY_ID", ACCESS_KEY);
    command.env("AWS_SECRET_ACCESS_KEY", SECRET_KEY);
    command.env("AWS_DEFAULT_REGION", "us-east-1");
    command.env("AWS_CONFIG_FILE", "/dev/null");
    command.env("AWS_SHARED_CREDENTIALS_FILE", "/dev/null");
    command.env_remove("AWS_PROFILE");
    command.env_remove("AWS_DEFAULT_PROFILE");
    command.env_remove("AWS_SESSION_TOKEN");
    command.env_remove("AWS_SECURITY_TOKEN");
    command.args([
        "-o",
        "s3.bucket-lookup=path",
        "-r",
        repository.as_str(),
        "restore",
        "latest",
        "--target",
    ]);
    command.arg(restore);
    let output = command.output().expect("restic");
    assert!(output.status.success(), "restic status {}", output.status);
}

fn restic_binary() -> PathBuf {
    if let Some(path) = std::env::var_os("RESTIC_BIN") {
        return PathBuf::from(path);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.satchel/bin/restic")
}

#[test]
fn remote_docker_host_is_rejected() {
    assert!(local_socket_host("unix:///var/run/docker.sock"));
    assert!(!local_socket_host("tcp://127.0.0.1:2375"));
    assert!(!local_socket_host("ssh://example"));
    assert!(!local_socket_host("http://127.0.0.1:2375"));
    assert!(!local_socket_host("https://127.0.0.1:2375"));
    assert!(!local_socket_host(""));
}
