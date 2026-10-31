//! JNI catalog for Android.
//!
//! `sampleCatalog` builds a private repository and returns metadata only.
//! `markSample` and `markStore` write that device's follow into a new snapshot.
//! `catalogGrant` opens with the device key from a pairing grant.
//! File bytes are available from `readFile` after a mark asks for them.

use std::fs;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use jni::EnvUnowned;
use jni::objects::{JClass, JString};
use jni::sys::{jbyteArray, jstring};
use satchel_core::{Client, DeviceId, Follow, Location, S3Store, init, read_followed};
use satchel_pair::JoinRequest;
use serde::{Deserialize, Serialize};

const PASSWORD: &str = "recovery-password";
const PUBLISHER: &str = "0123456789abcdef0123456789abcdef";
const READER: &str = "fedcba9876543210fedcba9876543210";
const PAYLOAD: &str = "payload-bytes";

#[derive(Debug)]
struct CatalogFailure;

impl std::fmt::Display for CatalogFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("catalog failed")
    }
}

impl std::error::Error for CatalogFailure {}

impl From<jni::errors::Error> for CatalogFailure {
    fn from(_error: jni::errors::Error) -> Self {
        CatalogFailure
    }
}

#[derive(Serialize)]
struct RowJson<'a> {
    path: &'a str,
    directory: bool,
    size: u64,
    #[serde(rename = "versionUnix")]
    version_unix: i64,
    devices: &'a [String],
    follow: Follow,
}

fn sample_catalog(root: &Path) -> Result<String, CatalogFailure> {
    let location = ensure_sample(root)?;
    let reader = DeviceId::parse(READER).map_err(|_| CatalogFailure)?;
    let client = Client::open(reader, root.join("work"), PASSWORD).map_err(|_| CatalogFailure)?;
    let entries = client.catalog(&location).map_err(|_| CatalogFailure)?;
    let json = rows_json(&entries)?;
    if json.contains(PAYLOAD) {
        return Err(CatalogFailure);
    }
    Ok(json)
}

fn rows_json(entries: &[satchel_core::CatalogEntry]) -> Result<String, CatalogFailure> {
    let mut rows = Vec::new();
    for entry in entries {
        rows.push(RowJson {
            path: &entry.path,
            directory: entry.is_directory,
            size: entry.size,
            version_unix: entry.version_unix,
            devices: &entry.devices,
            follow: entry.follow,
        });
    }
    serde_json::to_string(&rows).map_err(|_| CatalogFailure)
}

fn read_sample(root: &Path, path: &str) -> Result<Vec<u8>, CatalogFailure> {
    let location = ensure_sample(root)?;
    let reader = DeviceId::parse(READER).map_err(|_| CatalogFailure)?;
    read_followed(&location, PASSWORD, &reader, path).map_err(|_| CatalogFailure)
}

fn ensure_sample(root: &Path) -> Result<Location, CatalogFailure> {
    let repository = root.join("repository");
    let source = root.join("source");
    fs::create_dir_all(&repository).map_err(|_| CatalogFailure)?;
    fs::create_dir_all(&source).map_err(|_| CatalogFailure)?;
    fs::create_dir_all(root.join("work")).map_err(|_| CatalogFailure)?;
    let location = Location::Directory(repository.clone());
    if repository.join("config").exists() {
        return Ok(location);
    }
    fs::write(source.join("kept.txt"), PAYLOAD).map_err(|_| CatalogFailure)?;
    let device = DeviceId::parse(PUBLISHER).map_err(|_| CatalogFailure)?;
    init(&location, PASSWORD, &device).map_err(|_| CatalogFailure)?;
    let mut publisher = Client::open(device, &source, PASSWORD).map_err(|_| CatalogFailure)?;
    publisher
        .follow("kept.txt", Follow::Always)
        .map_err(|_| CatalogFailure)?;
    publisher
        .set_clock(1_700_000_000)
        .map_err(|_| CatalogFailure)?;
    publisher.sync(&location).map_err(|_| CatalogFailure)?;
    Ok(location)
}

#[derive(Deserialize)]
struct StoreSpec {
    bucket: String,
    endpoint: String,
    region: String,
    prefix: String,
    access_key_id: String,
    secret_access_key: String,
    password: String,
    work: String,
}

fn catalog_store(spec_path: &Path) -> Result<String, CatalogFailure> {
    let (location, spec) = open_store(spec_path)?;
    let reader = DeviceId::parse(READER).map_err(|_| CatalogFailure)?;
    let client = Client::open(reader, &spec.work, &spec.password).map_err(|_| CatalogFailure)?;
    let entries = client.catalog(&location).map_err(|_| CatalogFailure)?;
    rows_json(&entries)
}

fn mark_sample(root: &Path, path: &str, follow: &str) -> Result<(), CatalogFailure> {
    let follow = parse_follow(follow)?;
    let location = ensure_sample(root)?;
    publish_mark(&location, root.join("work"), PASSWORD, path, follow)
}

fn mark_store(spec_path: &Path, path: &str, follow: &str) -> Result<(), CatalogFailure> {
    let follow = parse_follow(follow)?;
    let (location, spec) = open_store(spec_path)?;
    publish_mark(
        &location,
        Path::new(&spec.work),
        &spec.password,
        path,
        follow,
    )
}

fn publish_mark(
    location: &Location,
    work: impl Into<std::path::PathBuf>,
    password: &str,
    path: &str,
    follow: Follow,
) -> Result<(), CatalogFailure> {
    let reader = DeviceId::parse(READER).map_err(|_| CatalogFailure)?;
    let mut client = Client::open(reader, work, password).map_err(|_| CatalogFailure)?;
    client.follow(path, follow).map_err(|_| CatalogFailure)?;
    client.sync(location).map_err(|_| CatalogFailure)?;
    Ok(())
}

fn parse_follow(text: &str) -> Result<Follow, CatalogFailure> {
    if text == "see" {
        return Ok(Follow::See);
    }
    if text == "once" {
        return Ok(Follow::Once);
    }
    if text == "always" {
        return Ok(Follow::Always);
    }
    Err(CatalogFailure)
}

fn read_store(spec_path: &Path, path: &str) -> Result<Vec<u8>, CatalogFailure> {
    let (location, spec) = open_store(spec_path)?;
    let reader = DeviceId::parse(READER).map_err(|_| CatalogFailure)?;
    read_followed(&location, &spec.password, &reader, path).map_err(|_| CatalogFailure)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GrantSpec {
    endpoint: String,
    bucket_credential: String,
    device_key_password: String,
    device_id: String,
    work: String,
    #[serde(default)]
    bucket: String,
    #[serde(default)]
    region: String,
    #[serde(default)]
    prefix: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AccessDoc {
    access_key_id: String,
    secret_access_key: String,
    #[serde(default)]
    bucket: String,
    #[serde(default)]
    region: String,
    #[serde(default)]
    prefix: String,
}

#[derive(Serialize)]
struct GrantWire<'a> {
    endpoint: &'a str,
    bucket_credential: &'a str,
    device_key_password: &'a str,
}

fn read_grant(path: &Path) -> Result<GrantSpec, CatalogFailure> {
    let text = fs::read_to_string(path).map_err(|_| CatalogFailure)?;
    serde_json::from_str(&text).map_err(|_| CatalogFailure)
}

fn open_grant(spec: &GrantSpec) -> Result<Location, CatalogFailure> {
    if spec.endpoint.starts_with("http://") || spec.endpoint.starts_with("https://") {
        let access: AccessDoc =
            serde_json::from_str(&spec.bucket_credential).map_err(|_| CatalogFailure)?;
        let bucket = if spec.bucket.is_empty() {
            access.bucket
        } else {
            spec.bucket.clone()
        };
        let region = if spec.region.is_empty() {
            access.region
        } else {
            spec.region.clone()
        };
        let prefix = if spec.prefix.is_empty() {
            access.prefix
        } else {
            spec.prefix.clone()
        };
        let store = S3Store::new(
            bucket,
            spec.endpoint.clone(),
            region,
            prefix,
            access.access_key_id,
            access.secret_access_key,
        )
        .map_err(|_| CatalogFailure)?;
        return Ok(Location::S3(store));
    }
    if spec.endpoint.is_empty() {
        return Err(CatalogFailure);
    }
    Ok(Location::Directory(PathBuf::from(&spec.endpoint)))
}

fn catalog_grant(spec_path: &Path) -> Result<String, CatalogFailure> {
    let spec = read_grant(spec_path)?;
    let location = open_grant(&spec)?;
    let device = DeviceId::parse(&spec.device_id).map_err(|_| CatalogFailure)?;
    let client =
        Client::open(device, &spec.work, &spec.device_key_password).map_err(|_| CatalogFailure)?;
    let entries = client.catalog(&location).map_err(|_| CatalogFailure)?;
    rows_json(&entries)
}

fn mark_grant(spec_path: &Path, path: &str, follow: &str) -> Result<(), CatalogFailure> {
    let follow = parse_follow(follow)?;
    let spec = read_grant(spec_path)?;
    let location = open_grant(&spec)?;
    let device = DeviceId::parse(&spec.device_id).map_err(|_| CatalogFailure)?;
    let mut client =
        Client::open(device, &spec.work, &spec.device_key_password).map_err(|_| CatalogFailure)?;
    client.follow(path, follow).map_err(|_| CatalogFailure)?;
    client.sync(&location).map_err(|_| CatalogFailure)?;
    Ok(())
}

fn read_grant_bytes(spec_path: &Path, path: &str) -> Result<Vec<u8>, CatalogFailure> {
    let spec = read_grant(spec_path)?;
    let location = open_grant(&spec)?;
    let device = DeviceId::parse(&spec.device_id).map_err(|_| CatalogFailure)?;
    read_followed(&location, &spec.device_key_password, &device, path).map_err(|_| CatalogFailure)
}

fn join_pairing(
    address: &str,
    code: &str,
    fingerprint: &str,
    device_name: &str,
    os: &str,
    device_id: &str,
    timeout: Duration,
) -> Result<String, CatalogFailure> {
    let address: SocketAddr = address.parse().map_err(|_| CatalogFailure)?;
    let secrets = satchel_pair::join(JoinRequest {
        address,
        code: code.to_string(),
        fingerprint: fingerprint.to_string(),
        device_name: device_name.to_string(),
        os: os.to_string(),
        device_id: device_id.to_string(),
        claimed_address: String::new(),
        claimed_interface: String::new(),
        timeout,
    })
    .map_err(|_| CatalogFailure)?;
    let wire = GrantWire {
        endpoint: &secrets.endpoint,
        bucket_credential: &secrets.bucket_credential,
        device_key_password: &secrets.device_key_password,
    };
    serde_json::to_string(&wire).map_err(|_| CatalogFailure)
}

fn open_store(spec_path: &Path) -> Result<(Location, StoreSpec), CatalogFailure> {
    let text = fs::read_to_string(spec_path).map_err(|_| CatalogFailure)?;
    let spec: StoreSpec = serde_json::from_str(&text).map_err(|_| CatalogFailure)?;
    let store = S3Store::new(
        spec.bucket.clone(),
        spec.endpoint.clone(),
        spec.region.clone(),
        spec.prefix.clone(),
        spec.access_key_id.clone(),
        spec.secret_access_key.clone(),
    )
    .map_err(|_| CatalogFailure)?;
    Ok((Location::S3(store), spec))
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_lucaswilliameufrasio_satchel_catalog_NativeCatalog_sampleCatalog<
    'local,
>(
    mut unowned: EnvUnowned<'local>,
    _class: JClass<'local>,
    root: JString<'local>,
) -> jstring {
    unowned
        .with_env(|env| -> Result<jstring, CatalogFailure> {
            let text = root.try_to_string(env)?;
            if text.is_empty() {
                return Err(CatalogFailure);
            }
            let json = sample_catalog(Path::new(&text))?;
            let value = env.new_string(json)?;
            Ok(value.into_raw())
        })
        .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_lucaswilliameufrasio_satchel_catalog_NativeCatalog_readFile<
    'local,
>(
    mut unowned: EnvUnowned<'local>,
    _class: JClass<'local>,
    root: JString<'local>,
    path: JString<'local>,
) -> jbyteArray {
    unowned
        .with_env(|env| -> Result<jbyteArray, CatalogFailure> {
            let root_text = root.try_to_string(env)?;
            let path_text = path.try_to_string(env)?;
            if root_text.is_empty() || path_text.is_empty() {
                return Err(CatalogFailure);
            }
            let bytes = read_sample(Path::new(&root_text), &path_text)?;
            let array = env.byte_array_from_slice(&bytes)?;
            Ok(array.into_raw())
        })
        .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_lucaswilliameufrasio_satchel_catalog_NativeCatalog_markSample<
    'local,
>(
    mut unowned: EnvUnowned<'local>,
    _class: JClass<'local>,
    root: JString<'local>,
    path: JString<'local>,
    follow: JString<'local>,
) {
    unowned
        .with_env(|env| -> Result<(), CatalogFailure> {
            let root_text = root.try_to_string(env)?;
            let path_text = path.try_to_string(env)?;
            let follow_text = follow.try_to_string(env)?;
            if root_text.is_empty() || path_text.is_empty() || follow_text.is_empty() {
                return Err(CatalogFailure);
            }
            mark_sample(Path::new(&root_text), &path_text, &follow_text)
        })
        .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_lucaswilliameufrasio_satchel_catalog_NativeCatalog_markStore<
    'local,
>(
    mut unowned: EnvUnowned<'local>,
    _class: JClass<'local>,
    spec: JString<'local>,
    path: JString<'local>,
    follow: JString<'local>,
) {
    unowned
        .with_env(|env| -> Result<(), CatalogFailure> {
            let spec_text = spec.try_to_string(env)?;
            let path_text = path.try_to_string(env)?;
            let follow_text = follow.try_to_string(env)?;
            if spec_text.is_empty() || path_text.is_empty() || follow_text.is_empty() {
                return Err(CatalogFailure);
            }
            mark_store(Path::new(&spec_text), &path_text, &follow_text)
        })
        .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_lucaswilliameufrasio_satchel_catalog_NativeCatalog_catalogStore<
    'local,
>(
    mut unowned: EnvUnowned<'local>,
    _class: JClass<'local>,
    spec: JString<'local>,
) -> jstring {
    unowned
        .with_env(|env| -> Result<jstring, CatalogFailure> {
            let text = spec.try_to_string(env)?;
            if text.is_empty() {
                return Err(CatalogFailure);
            }
            let json = catalog_store(Path::new(&text))?;
            let value = env.new_string(json)?;
            Ok(value.into_raw())
        })
        .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_lucaswilliameufrasio_satchel_catalog_NativeCatalog_readStore<
    'local,
>(
    mut unowned: EnvUnowned<'local>,
    _class: JClass<'local>,
    spec: JString<'local>,
    path: JString<'local>,
) -> jbyteArray {
    unowned
        .with_env(|env| -> Result<jbyteArray, CatalogFailure> {
            let spec_text = spec.try_to_string(env)?;
            let path_text = path.try_to_string(env)?;
            if spec_text.is_empty() || path_text.is_empty() {
                return Err(CatalogFailure);
            }
            let bytes = read_store(Path::new(&spec_text), &path_text)?;
            let array = env.byte_array_from_slice(&bytes)?;
            Ok(array.into_raw())
        })
        .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_lucaswilliameufrasio_satchel_catalog_NativeCatalog_catalogGrant<
    'local,
>(
    mut unowned: EnvUnowned<'local>,
    _class: JClass<'local>,
    spec: JString<'local>,
) -> jstring {
    unowned
        .with_env(|env| -> Result<jstring, CatalogFailure> {
            let text = spec.try_to_string(env)?;
            if text.is_empty() {
                return Err(CatalogFailure);
            }
            let json = catalog_grant(Path::new(&text))?;
            let value = env.new_string(json)?;
            Ok(value.into_raw())
        })
        .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_lucaswilliameufrasio_satchel_catalog_NativeCatalog_markGrant<
    'local,
>(
    mut unowned: EnvUnowned<'local>,
    _class: JClass<'local>,
    spec: JString<'local>,
    path: JString<'local>,
    follow: JString<'local>,
) {
    unowned
        .with_env(|env| -> Result<(), CatalogFailure> {
            let spec_text = spec.try_to_string(env)?;
            let path_text = path.try_to_string(env)?;
            let follow_text = follow.try_to_string(env)?;
            if spec_text.is_empty() || path_text.is_empty() || follow_text.is_empty() {
                return Err(CatalogFailure);
            }
            mark_grant(Path::new(&spec_text), &path_text, &follow_text)
        })
        .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_lucaswilliameufrasio_satchel_catalog_NativeCatalog_readGrant<
    'local,
>(
    mut unowned: EnvUnowned<'local>,
    _class: JClass<'local>,
    spec: JString<'local>,
    path: JString<'local>,
) -> jbyteArray {
    unowned
        .with_env(|env| -> Result<jbyteArray, CatalogFailure> {
            let spec_text = spec.try_to_string(env)?;
            let path_text = path.try_to_string(env)?;
            if spec_text.is_empty() || path_text.is_empty() {
                return Err(CatalogFailure);
            }
            let bytes = read_grant_bytes(Path::new(&spec_text), &path_text)?;
            let array = env.byte_array_from_slice(&bytes)?;
            Ok(array.into_raw())
        })
        .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_lucaswilliameufrasio_satchel_catalog_NativeCatalog_joinPairing<
    'local,
>(
    mut unowned: EnvUnowned<'local>,
    _class: JClass<'local>,
    address: JString<'local>,
    code: JString<'local>,
    fingerprint: JString<'local>,
    device_name: JString<'local>,
    os: JString<'local>,
    device_id: JString<'local>,
) -> jstring {
    unowned
        .with_env(|env| -> Result<jstring, CatalogFailure> {
            let address_text = address.try_to_string(env)?;
            let code_text = code.try_to_string(env)?;
            let fingerprint_text = fingerprint.try_to_string(env)?;
            let name_text = device_name.try_to_string(env)?;
            let os_text = os.try_to_string(env)?;
            let id_text = device_id.try_to_string(env)?;
            if address_text.is_empty()
                || code_text.is_empty()
                || fingerprint_text.is_empty()
                || name_text.is_empty()
                || os_text.is_empty()
                || id_text.is_empty()
            {
                return Err(CatalogFailure);
            }
            let json = join_pairing(
                &address_text,
                &code_text,
                &fingerprint_text,
                &name_text,
                &os_text,
                &id_text,
                satchel_pair::DEFAULT_TIMEOUT,
            )?;
            let value = env.new_string(json)?;
            Ok(value.into_raw())
        })
        .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_lucaswilliameufrasio_satchel_catalog_NativeCatalog_newDeviceId<
    'local,
>(
    mut unowned: EnvUnowned<'local>,
    _class: JClass<'local>,
) -> jstring {
    unowned
        .with_env(|env| -> Result<jstring, CatalogFailure> {
            let id = DeviceId::generate();
            let value = env.new_string(id.as_str())?;
            Ok(value.into_raw())
        })
        .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{PAYLOAD, READER, mark_sample, read_sample, sample_catalog};

    struct TempRoot(PathBuf);

    impl Drop for TempRoot {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn sample_lists_metadata_and_keeps_bytes_out_of_the_json() {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let root = TempRoot(std::env::temp_dir().join(format!("satchel-ffi-{nanos}")));
        let json = sample_catalog(&root.0).expect("catalog");
        assert!(json.contains("\"path\":\"kept.txt\""));
        assert!(json.contains("\"follow\":\"see\""));
        assert!(!json.contains(PAYLOAD), "catalog json includes file bytes");
        assert!(!root.0.join("work").join("kept.txt").exists());
        let bytes = read_sample(&root.0, "kept.txt").expect("read");
        assert_eq!(bytes, PAYLOAD.as_bytes());
        assert!(!root.0.join("work").join("kept.txt").exists());
        assert!(mark_sample(&root.0, "kept.txt", "nope").is_err());
        mark_sample(&root.0, "kept.txt", "once").expect("mark");
        assert_eq!(
            fs::read(root.0.join("work").join("kept.txt")).expect("work"),
            PAYLOAD.as_bytes()
        );
        let marked = sample_catalog(&root.0).expect("marked catalog");
        assert!(marked.contains("\"follow\":\"once\""));
        assert!(marked.contains(READER));
        assert!(
            !marked.contains(PAYLOAD),
            "marked catalog includes file bytes"
        );
    }

    #[test]
    fn grant_opens_with_the_device_password() {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;

        use satchel_core::{DeviceId, Location, add_device_key, publish_folder};

        use super::{catalog_grant, mark_grant, read_grant};

        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let root = TempRoot(std::env::temp_dir().join(format!("satchel-grant-{nanos}")));
        let repository = root.0.join("repository");
        let source = root.0.join("source");
        let work = root.0.join("work");
        fs::create_dir_all(&repository).expect("repository");
        fs::create_dir_all(source.join("notes")).expect("source");
        fs::write(source.join("notes/kept.txt"), PAYLOAD).expect("file");
        let publisher = DeviceId::parse(super::PUBLISHER).expect("publisher");
        publish_folder(
            &Location::Directory(repository.clone()),
            super::PASSWORD,
            &publisher,
            &source,
        )
        .expect("publish");
        let reader = DeviceId::parse(READER).expect("reader");
        add_device_key(
            &Location::Directory(repository.clone()),
            super::PASSWORD,
            &reader,
            "device-key-password",
        )
        .expect("device key");
        let grant_path = root.0.join("grant.json");
        let body = serde_json::json!({
            "endpoint": repository,
            "bucket_credential": "bucket-fixture",
            "device_key_password": "device-key-password",
            "device_id": READER,
            "work": work,
        });
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&grant_path)
            .expect("grant file");
        file.write_all(body.to_string().as_bytes()).expect("write");
        drop(file);
        let json = catalog_grant(&grant_path).expect("catalog");
        assert!(json.contains("\"follow\":\"see\""));
        assert!(json.contains("notes/kept.txt"));
        assert!(!json.contains(PAYLOAD));
        assert!(!json.contains("device-key-password"));
        assert!(!json.contains(super::PASSWORD));
        assert!(!work.join("notes/kept.txt").exists());
        mark_grant(&grant_path, "notes/kept.txt", "once").expect("mark");
        assert_eq!(
            fs::read(work.join("notes/kept.txt")).expect("work"),
            PAYLOAD.as_bytes()
        );
        let bytes = super::read_grant_bytes(&grant_path, "notes/kept.txt").expect("read");
        assert_eq!(bytes, PAYLOAD.as_bytes());
        let rejected = root.0.join("rejected.json");
        fs::write(
            &rejected,
            r#"{"endpoint":"/tmp","bucket_credential":"bucket-fixture","device_key_password":"device-key-password","device_id":"fedcba9876543210fedcba9876543210","work":"/tmp","recovery":"no"}"#,
        )
        .expect("rejected");
        assert!(read_grant(&rejected).is_err());
    }

    #[test]
    fn join_returns_the_grant_without_a_recovery_field() {
        use std::thread;
        use std::time::{Duration, Instant};

        use satchel_pair::{Decision, DeviceSecrets, ListenConfig, PairingSession};

        use super::join_pairing;

        let session = PairingSession::listen(ListenConfig {
            bind: "127.0.0.1:0".parse().expect("bind"),
            timeout: Duration::from_secs(5),
            advertise: false,
        })
        .expect("listen");
        let offer = session.offer().clone();
        let address = offer.address.to_string();
        let joiner = thread::spawn(move || {
            join_pairing(
                &address,
                &offer.code,
                &offer.fingerprint,
                "phone",
                "android",
                super::READER,
                Duration::from_secs(5),
            )
        });
        let deadline = Instant::now() + Duration::from_secs(5);
        while session.try_approval().expect("poll").is_none() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(20));
        }
        session
            .decide(Decision::Approve(DeviceSecrets {
                endpoint: "http://127.0.0.1:9".to_string(),
                bucket_credential: "bucket-fixture".to_string(),
                device_key_password: "device-key-password".to_string(),
            }))
            .expect("approve");
        let json = joiner.join().expect("thread").expect("grant");
        let value: serde_json::Value = serde_json::from_str(&json).expect("json");
        let mut keys = Vec::new();
        for key in value.as_object().expect("object").keys() {
            keys.push(key.clone());
        }
        keys.sort();
        assert_eq!(
            keys,
            vec![
                "bucket_credential".to_string(),
                "device_key_password".to_string(),
                "endpoint".to_string()
            ]
        );
        assert_eq!(value["endpoint"].as_str(), Some("http://127.0.0.1:9"));
        assert_eq!(value["bucket_credential"].as_str(), Some("bucket-fixture"));
        assert_eq!(
            value["device_key_password"].as_str(),
            Some("device-key-password")
        );
        assert!(!json.contains("recovery"));
    }
}
