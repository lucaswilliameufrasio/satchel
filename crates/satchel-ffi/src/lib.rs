//! JNI catalog for Android.
//!
//! `sampleCatalog` builds a private repository and returns metadata only.
//! `markSample` and `markStore` write that device's follow into a new snapshot.
//! `catalogGrant` opens with the device key from a pairing grant.
//! File bytes are available from `readFile` after a mark asks for them.
//!
//! The same functions build for iOS and for the host tests. The JNI symbols
//! stay on Android.

#![cfg(any(test, target_os = "android", target_os = "ios"))]

use std::fs;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[cfg(target_os = "android")]
use jni::EnvUnowned;
#[cfg(target_os = "android")]
use jni::objects::{JClass, JString};
#[cfg(target_os = "android")]
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

#[cfg(target_os = "android")]
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
    holders: &'a [satchel_core::DeviceCard],
    follow: Follow,
}

/// The example lives in a cache the app can lose at any moment. A run killed halfway
/// leaves a repository that never opens again, so one failed open rebuilds it.
fn sample_catalog(root: &Path) -> Result<String, CatalogFailure> {
    // A half-built example can open fine and list nothing, so an empty list counts as broken too.
    match sample_catalog_once(root).and_then(|json| {
        if json.contains("\"path\":\"kept.txt\"") {
            Ok(json)
        } else {
            Err(CatalogFailure)
        }
    }) {
        Ok(json) => Ok(json),
        Err(_) => {
            for part in ["repository", "source", "work"] {
                let _ = fs::remove_dir_all(root.join(part));
            }
            sample_catalog_once(root)
        }
    }
}

fn sample_catalog_once(root: &Path) -> Result<String, CatalogFailure> {
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
            holders: &entry.holders,
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
        .introduce("Publicador", "exemplo")
        .map_err(|_| CatalogFailure)?;
    publisher
        .follow("kept.txt", Follow::Always)
        .map_err(|_| CatalogFailure)?;
    publisher
        .set_clock(1_700_000_000)
        .map_err(|_| CatalogFailure)?;
    publisher.sync(&location).map_err(|_| CatalogFailure)?;
    Ok(location)
}

#[cfg(target_os = "android")]
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

#[cfg(target_os = "android")]
fn catalog_store(spec_path: &Path) -> Result<String, CatalogFailure> {
    let (location, spec) = open_store(spec_path)?;
    let reader = DeviceId::parse(READER).map_err(|_| CatalogFailure)?;
    let client = Client::open(reader, &spec.work, &spec.password).map_err(|_| CatalogFailure)?;
    let entries = client.catalog(&location).map_err(|_| CatalogFailure)?;
    rows_json(&entries)
}

fn mark_sample(
    root: &Path,
    path: &str,
    follow: &str,
    name: &str,
    os: &str,
) -> Result<(), CatalogFailure> {
    let follow = parse_follow(follow)?;
    let location = ensure_sample(root)?;
    publish_mark(
        &location,
        root.join("work"),
        PASSWORD,
        path,
        follow,
        name,
        os,
    )
}

#[cfg(target_os = "android")]
fn mark_store(
    spec_path: &Path,
    path: &str,
    follow: &str,
    name: &str,
    os: &str,
) -> Result<(), CatalogFailure> {
    let follow = parse_follow(follow)?;
    let (location, spec) = open_store(spec_path)?;
    publish_mark(
        &location,
        Path::new(&spec.work),
        &spec.password,
        path,
        follow,
        name,
        os,
    )
}

fn publish_mark(
    location: &Location,
    work: impl Into<std::path::PathBuf>,
    password: &str,
    path: &str,
    follow: Follow,
    name: &str,
    os: &str,
) -> Result<(), CatalogFailure> {
    let reader = DeviceId::parse(READER).map_err(|_| CatalogFailure)?;
    let mut client = Client::open(reader, work, password).map_err(|_| CatalogFailure)?;
    if !name.is_empty() || !os.is_empty() {
        client.introduce(name, os).map_err(|_| CatalogFailure)?;
    }
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

#[cfg(target_os = "android")]
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
    /// `maintainer` on the device that created the repository. Empty on a device that joined.
    #[serde(default)]
    role: String,
    /// Credential sent to a device that joins. Only a maintainer file carries it.
    #[serde(default)]
    limited_credential: String,
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

/// The device key password and the stored credentials are cleared when the grant is dropped.
impl Drop for GrantSpec {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.device_key_password.zeroize();
        self.bucket_credential.zeroize();
        self.limited_credential.zeroize();
    }
}

/// A grant is a file path, or the grant JSON itself when the phone keeps it in the
/// system keystore and never writes it to disk. Inline text starts with `{`.
fn read_grant(path: &Path) -> Result<GrantSpec, CatalogFailure> {
    let named = path.to_str().ok_or(CatalogFailure)?;
    if named.trim_start().starts_with('{') {
        return serde_json::from_str(named).map_err(|_| CatalogFailure);
    }
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

fn mark_grant(
    spec_path: &Path,
    path: &str,
    follow: &str,
    name: &str,
    os: &str,
) -> Result<(), CatalogFailure> {
    let follow = parse_follow(follow)?;
    let spec = read_grant(spec_path)?;
    let location = open_grant(&spec)?;
    let device = DeviceId::parse(&spec.device_id).map_err(|_| CatalogFailure)?;
    let mut client =
        Client::open(device, &spec.work, &spec.device_key_password).map_err(|_| CatalogFailure)?;
    if !name.is_empty() || !os.is_empty() {
        client.introduce(name, os).map_err(|_| CatalogFailure)?;
    }
    client.follow(path, follow).map_err(|_| CatalogFailure)?;
    client.sync(&location).map_err(|_| CatalogFailure)?;
    Ok(())
}

/// One folder round through a grant. `mark` sets a follow first, as `mark_grant` does.
///
/// Returns report JSON. `"status":"unavailable"` means the folder listed nothing
/// while the last round saw files, so nothing was deleted anywhere.
fn folder_grant(
    spec_path: &Path,
    tree: &mut dyn satchel_core::FolderTree,
    manifest: &Path,
    mark: Option<(&str, &str)>,
    name: &str,
    os: &str,
) -> Result<String, CatalogFailure> {
    let spec = read_grant(spec_path)?;
    let location = open_grant(&spec)?;
    let device = DeviceId::parse(&spec.device_id).map_err(|_| CatalogFailure)?;
    let mut client =
        Client::open(device, &spec.work, &spec.device_key_password).map_err(|_| CatalogFailure)?;
    if !name.is_empty() || !os.is_empty() {
        client.introduce(name, os).map_err(|_| CatalogFailure)?;
    }
    if let Some((path, follow)) = mark {
        client
            .follow(path, parse_follow(follow)?)
            .map_err(|_| CatalogFailure)?;
    }
    match satchel_core::sync_mirrored(&mut client, &location, tree, manifest) {
        Ok(report) => serde_json::to_string(&FolderReportJson {
            status: "ok",
            imported: report.imported.len(),
            exported: report.exported.len(),
            deleted_here: report.deleted_here.len(),
            deleted_there: report.deleted_there.len(),
            deferred: report.deferred.len(),
            conflicts: report.outcome.conflicts.len(),
        })
        .map_err(|_| CatalogFailure),
        Err(satchel_core::Error::FolderUnavailable) => serde_json::to_string(&FolderReportJson {
            status: "unavailable",
            ..FolderReportJson::default()
        })
        .map_err(|_| CatalogFailure),
        Err(_) => Err(CatalogFailure),
    }
}

#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct FolderReportJson {
    status: &'static str,
    imported: usize,
    exported: usize,
    deleted_here: usize,
    deleted_there: usize,
    deferred: usize,
    conflicts: usize,
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

/// Canonical invite JSON. Unknown fields, including a recovery password, are rejected.
fn parse_invite(text: &str) -> Result<String, CatalogFailure> {
    let invite = satchel_pair::Invite::decode(text).map_err(|_| CatalogFailure)?;
    invite.encode().map_err(|_| CatalogFailure)
}

#[cfg(any(test, target_os = "ios"))]
/// Grant file the catalog opens. The joined text has no device id and no work path.
fn assemble_grant_json(
    joined: &str,
    device_id: &str,
    work: &str,
) -> Result<String, CatalogFailure> {
    if DeviceId::parse(device_id).is_err() || work.is_empty() {
        return Err(CatalogFailure);
    }
    let parsed: GrantWireOwned = serde_json::from_str(joined).map_err(|_| CatalogFailure)?;
    if parsed.endpoint.is_empty()
        || parsed.bucket_credential.is_empty()
        || parsed.device_key_password.is_empty()
    {
        return Err(CatalogFailure);
    }
    let stored = StoredGrant {
        endpoint: parsed.endpoint,
        bucket_credential: parsed.bucket_credential,
        device_key_password: parsed.device_key_password,
        device_id: device_id.to_string(),
        work: work.to_string(),
    };
    serde_json::to_string(&stored).map_err(|_| CatalogFailure)
}

#[cfg(any(test, target_os = "ios"))]
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GrantWireOwned {
    endpoint: String,
    bucket_credential: String,
    device_key_password: String,
}

#[cfg(any(test, target_os = "ios"))]
#[derive(Serialize)]
struct StoredGrant {
    endpoint: String,
    bucket_credential: String,
    device_key_password: String,
    device_id: String,
    work: String,
}

#[cfg(target_os = "android")]
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

#[cfg(target_os = "android")]
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

#[cfg(target_os = "android")]
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

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_lucaswilliameufrasio_satchel_catalog_NativeCatalog_markSample<
    'local,
>(
    mut unowned: EnvUnowned<'local>,
    _class: JClass<'local>,
    root: JString<'local>,
    path: JString<'local>,
    follow: JString<'local>,
    name: JString<'local>,
    os: JString<'local>,
) {
    unowned
        .with_env(|env| -> Result<(), CatalogFailure> {
            let root_text = root.try_to_string(env)?;
            let path_text = path.try_to_string(env)?;
            let follow_text = follow.try_to_string(env)?;
            let name_text = name.try_to_string(env)?;
            let os_text = os.try_to_string(env)?;
            if root_text.is_empty() || path_text.is_empty() || follow_text.is_empty() {
                return Err(CatalogFailure);
            }
            mark_sample(
                Path::new(&root_text),
                &path_text,
                &follow_text,
                &name_text,
                &os_text,
            )
        })
        .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_lucaswilliameufrasio_satchel_catalog_NativeCatalog_markStore<
    'local,
>(
    mut unowned: EnvUnowned<'local>,
    _class: JClass<'local>,
    spec: JString<'local>,
    path: JString<'local>,
    follow: JString<'local>,
    name: JString<'local>,
    os: JString<'local>,
) {
    unowned
        .with_env(|env| -> Result<(), CatalogFailure> {
            let spec_text = spec.try_to_string(env)?;
            let path_text = path.try_to_string(env)?;
            let follow_text = follow.try_to_string(env)?;
            let name_text = name.try_to_string(env)?;
            let os_text = os.try_to_string(env)?;
            if spec_text.is_empty() || path_text.is_empty() || follow_text.is_empty() {
                return Err(CatalogFailure);
            }
            mark_store(
                Path::new(&spec_text),
                &path_text,
                &follow_text,
                &name_text,
                &os_text,
            )
        })
        .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}

#[cfg(target_os = "android")]
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

#[cfg(target_os = "android")]
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

#[cfg(target_os = "android")]
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

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_lucaswilliameufrasio_satchel_catalog_NativeCatalog_markGrant<
    'local,
>(
    mut unowned: EnvUnowned<'local>,
    _class: JClass<'local>,
    spec: JString<'local>,
    path: JString<'local>,
    follow: JString<'local>,
    name: JString<'local>,
    os: JString<'local>,
) {
    unowned
        .with_env(|env| -> Result<(), CatalogFailure> {
            let spec_text = spec.try_to_string(env)?;
            let path_text = path.try_to_string(env)?;
            let follow_text = follow.try_to_string(env)?;
            let name_text = name.try_to_string(env)?;
            let os_text = os.try_to_string(env)?;
            if spec_text.is_empty() || path_text.is_empty() || follow_text.is_empty() {
                return Err(CatalogFailure);
            }
            mark_grant(
                Path::new(&spec_text),
                &path_text,
                &follow_text,
                &name_text,
                &os_text,
            )
        })
        .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}

#[cfg(target_os = "android")]
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

#[cfg(target_os = "android")]
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

#[cfg(target_os = "android")]
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

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_lucaswilliameufrasio_satchel_catalog_NativeCatalog_parseInvite<
    'local,
>(
    mut unowned: EnvUnowned<'local>,
    _class: JClass<'local>,
    text: JString<'local>,
) -> jstring {
    unowned
        .with_env(|env| -> Result<jstring, CatalogFailure> {
            let raw = text.try_to_string(env)?;
            if raw.is_empty() {
                return Err(CatalogFailure);
            }
            let json = parse_invite(&raw)?;
            let value = env.new_string(json)?;
            Ok(value.into_raw())
        })
        .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}

#[cfg(target_os = "android")]
fn text_arg(
    env: &mut jni::Env<'_>,
    value: &JString<'_>,
    allow_empty: bool,
) -> Result<String, CatalogFailure> {
    let text = value.try_to_string(env)?;
    if text.is_empty() && !allow_empty {
        return Err(CatalogFailure);
    }
    Ok(text)
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_lucaswilliameufrasio_satchel_catalog_NativeCatalog_createRepository<
    'local,
>(
    mut unowned: EnvUnowned<'local>,
    _class: JClass<'local>,
    spec: JString<'local>,
    work: JString<'local>,
) -> jstring {
    unowned
        .with_env(|env| -> Result<jstring, CatalogFailure> {
            let spec_text = text_arg(env, &spec, false)?;
            let work_text = text_arg(env, &work, false)?;
            let created = maker::create_phone_repository(&spec_text, Path::new(&work_text))?;
            Ok(env.new_string(created)?.into_raw())
        })
        .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_lucaswilliameufrasio_satchel_catalog_NativeCatalog_createFailure<
    'local,
>(
    mut unowned: EnvUnowned<'local>,
    _class: JClass<'local>,
) -> jstring {
    unowned
        .with_env(|env| -> Result<jstring, CatalogFailure> {
            Ok(env.new_string(maker::last_failure())?.into_raw())
        })
        .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_lucaswilliameufrasio_satchel_catalog_NativeCatalog_isMaintainer<
    'local,
>(
    mut unowned: EnvUnowned<'local>,
    _class: JClass<'local>,
    grant: JString<'local>,
) -> jni::sys::jboolean {
    unowned
        .with_env(|env| -> Result<jni::sys::jboolean, CatalogFailure> {
            let grant_text = text_arg(env, &grant, false)?;
            Ok(maker::is_maintainer(Path::new(&grant_text)))
        })
        .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_lucaswilliameufrasio_satchel_catalog_NativeCatalog_holdStart<
    'local,
>(
    mut unowned: EnvUnowned<'local>,
    _class: JClass<'local>,
    grant: JString<'local>,
    bind: JString<'local>,
) -> jstring {
    unowned
        .with_env(|env| -> Result<jstring, CatalogFailure> {
            let grant_text = text_arg(env, &grant, false)?;
            let bind_text = text_arg(env, &bind, true)?;
            let offer = maker::hold_start(
                Path::new(&grant_text),
                &bind_text,
                satchel_pair::DEFAULT_TIMEOUT,
            )?;
            Ok(env.new_string(offer)?.into_raw())
        })
        .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_lucaswilliameufrasio_satchel_catalog_NativeCatalog_holdPoll<
    'local,
>(
    mut unowned: EnvUnowned<'local>,
    _class: JClass<'local>,
) -> jstring {
    unowned
        .with_env(|env| -> Result<jstring, CatalogFailure> {
            let shown = maker::hold_poll()?;
            Ok(env.new_string(shown)?.into_raw())
        })
        .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_lucaswilliameufrasio_satchel_catalog_NativeCatalog_holdDecide<
    'local,
>(
    mut unowned: EnvUnowned<'local>,
    _class: JClass<'local>,
    grant: JString<'local>,
    approve: jni::sys::jboolean,
) {
    unowned
        .with_env(|env| -> Result<(), CatalogFailure> {
            let grant_text = text_arg(env, &grant, false)?;
            maker::hold_decide(Path::new(&grant_text), approve)
        })
        .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_lucaswilliameufrasio_satchel_catalog_NativeCatalog_holdClose<
    'local,
>(
    mut unowned: EnvUnowned<'local>,
    _class: JClass<'local>,
) {
    unowned
        .with_env(|_env| -> Result<(), CatalogFailure> {
            maker::hold_close();
            Ok(())
        })
        .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_lucaswilliameufrasio_satchel_catalog_NativeCatalog_invitePng<
    'local,
>(
    mut unowned: EnvUnowned<'local>,
    _class: JClass<'local>,
    invite: JString<'local>,
) -> jbyteArray {
    unowned
        .with_env(|env| -> Result<jbyteArray, CatalogFailure> {
            let text = text_arg(env, &invite, false)?;
            let bytes = maker::invite_png(&text)?;
            Ok(env.byte_array_from_slice(&bytes)?.into_raw())
        })
        .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_lucaswilliameufrasio_satchel_catalog_NativeCatalog_recoveryPrepare<
    'local,
>(
    mut unowned: EnvUnowned<'local>,
    _class: JClass<'local>,
    grant: JString<'local>,
) -> jstring {
    unowned
        .with_env(|env| -> Result<jstring, CatalogFailure> {
            let grant_text = text_arg(env, &grant, false)?;
            let prepared = maker::recovery_prepare(&grant_text)?;
            Ok(env.new_string(prepared)?.into_raw())
        })
        .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_lucaswilliameufrasio_satchel_catalog_NativeCatalog_recoveryCommit<
    'local,
>(
    mut unowned: EnvUnowned<'local>,
    _class: JClass<'local>,
    old: JString<'local>,
    new: JString<'local>,
) -> jni::sys::jboolean {
    unowned
        .with_env(|env| -> Result<jni::sys::jboolean, CatalogFailure> {
            let old_text = text_arg(env, &old, false)?;
            let new_text = text_arg(env, &new, false)?;
            Ok(maker::recovery_commit(&old_text, &new_text).is_ok())
        })
        .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_lucaswilliameufrasio_satchel_catalog_NativeCatalog_grantOpens<
    'local,
>(
    mut unowned: EnvUnowned<'local>,
    _class: JClass<'local>,
    grant: JString<'local>,
) -> jni::sys::jboolean {
    unowned
        .with_env(|env| -> Result<jni::sys::jboolean, CatalogFailure> {
            let grant_text = text_arg(env, &grant, false)?;
            Ok(maker::grant_opens(&grant_text))
        })
        .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}

/// The Kotlin `FolderTree` the Storage Access Framework folder implements.
#[cfg(target_os = "android")]
struct JniTree<'a, 'env, 'obj> {
    env: &'a mut jni::Env<'env>,
    tree: &'a jni::objects::JObject<'obj>,
}

#[cfg(target_os = "android")]
impl JniTree<'_, '_, '_> {
    /// A thrown Kotlin exception becomes an error here and is cleared, so the
    /// caller decides what the app sees.
    fn failed(&mut self) -> satchel_core::Error {
        if self.env.exception_check() {
            self.env.exception_clear();
        }
        satchel_core::Error::FolderUnavailable
    }
}

#[cfg(target_os = "android")]
#[derive(Deserialize)]
struct TreeFileJson {
    path: String,
    size: u64,
    modified: i64,
}

#[cfg(target_os = "android")]
impl satchel_core::FolderTree for JniTree<'_, '_, '_> {
    fn list(&mut self) -> Result<Vec<satchel_core::TreeFile>, satchel_core::Error> {
        let text = (|| -> Result<String, jni::errors::Error> {
            let value = self.env.call_method(
                self.tree,
                jni::jni_str!("list"),
                jni::jni_sig!("()Ljava/lang/String;"),
                &[],
            )?;
            let object = value.l()?;
            let string = self.env.cast_local::<JString>(object)?;
            string.try_to_string(self.env)
        })();
        let Ok(text) = text else {
            return Err(self.failed());
        };
        let files: Vec<TreeFileJson> = serde_json::from_str(&text).map_err(|_| self.failed())?;
        Ok(files
            .into_iter()
            .map(|file| satchel_core::TreeFile {
                path: file.path,
                size: file.size,
                modified: file.modified,
            })
            .collect())
    }

    fn read(&mut self, path: &str) -> Result<Vec<u8>, satchel_core::Error> {
        let bytes = (|| -> Result<Vec<u8>, jni::errors::Error> {
            let name = self.env.new_string(path)?;
            let value = self.env.call_method(
                self.tree,
                jni::jni_str!("read"),
                jni::jni_sig!("(Ljava/lang/String;)[B"),
                &[(&name).into()],
            )?;
            let object = value.l()?;
            let array = self.env.cast_local::<jni::objects::JByteArray>(object)?;
            self.env.convert_byte_array(&array)
        })();
        bytes.map_err(|_| self.failed())
    }

    fn write(&mut self, path: &str, bytes: &[u8]) -> Result<(), satchel_core::Error> {
        let done = (|| -> Result<(), jni::errors::Error> {
            let name = self.env.new_string(path)?;
            let array = self.env.byte_array_from_slice(bytes)?;
            self.env.call_method(
                self.tree,
                jni::jni_str!("write"),
                jni::jni_sig!("(Ljava/lang/String;[B)V"),
                &[(&name).into(), (&array).into()],
            )?;
            Ok(())
        })();
        done.map_err(|_| self.failed())
    }

    fn remove(&mut self, path: &str) -> Result<(), satchel_core::Error> {
        let done = (|| -> Result<(), jni::errors::Error> {
            let name = self.env.new_string(path)?;
            self.env.call_method(
                self.tree,
                jni::jni_str!("remove"),
                jni::jni_sig!("(Ljava/lang/String;)V"),
                &[(&name).into()],
            )?;
            Ok(())
        })();
        done.map_err(|_| self.failed())
    }
}

/// One folder round. `path` and `follow` may be empty; when set they mark first.
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_lucaswilliameufrasio_satchel_catalog_NativeCatalog_syncFolder<
    'local,
>(
    mut unowned: EnvUnowned<'local>,
    _class: JClass<'local>,
    grant: JString<'local>,
    tree: jni::objects::JObject<'local>,
    manifest: JString<'local>,
    path: JString<'local>,
    follow: JString<'local>,
    name: JString<'local>,
    os: JString<'local>,
) -> jstring {
    unowned
        .with_env(|env| -> Result<jstring, CatalogFailure> {
            let grant_text = text_arg(env, &grant, false)?;
            let manifest_text = text_arg(env, &manifest, false)?;
            let path_text = text_arg(env, &path, true)?;
            let follow_text = text_arg(env, &follow, true)?;
            let name_text = text_arg(env, &name, true)?;
            let os_text = text_arg(env, &os, true)?;
            let mark = if path_text.is_empty() {
                None
            } else {
                Some((path_text.as_str(), follow_text.as_str()))
            };
            let json = {
                let mut folder = JniTree { env, tree: &tree };
                folder_grant(
                    Path::new(&grant_text),
                    &mut folder,
                    Path::new(&manifest_text),
                    mark,
                    &name_text,
                    &os_text,
                )?
            };
            let value = env.new_string(json)?;
            Ok(value.into_raw())
        })
        .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}

#[cfg(any(test, target_os = "ios"))]
mod ios;
mod maker;

#[cfg(any(test, target_os = "ios"))]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{PAYLOAD, READER, mark_sample, read_sample, sample_catalog};

    #[cfg(test)]
    use std::ffi::CString;
    #[cfg(test)]
    use std::ptr;

    #[cfg(test)]
    use super::ios::satchel_mark_sample;

    #[cfg(test)]
    #[test]
    fn parse_invite_returns_the_fields_and_rejects_recovery() {
        let text = r#"{"v":1,"code":"482193","address":"127.0.0.1:9","fingerprint":"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"}"#;
        let parsed = super::parse_invite(text).expect("invite");
        assert!(parsed.contains("127.0.0.1:9"));
        assert!(parsed.contains("482193"));
        assert!(!parsed.contains("recovery"));
        let bad = text.trim_end_matches('}').to_string() + r#","recovery":"nope"}"#;
        assert!(super::parse_invite(&bad).is_err());
        let joined = concat!(
            r#"{"endpoint":"http://127.0.0.1:9","bucket_credential":"phone-fixture","#,
            r#""device_key_password":"device-key-password"}"#
        );
        let stored =
            super::assemble_grant_json(joined, "fedcba9876543210fedcba9876543210", "/data/work")
                .expect("grant");
        assert_eq!(
            stored,
            concat!(
                r#"{"endpoint":"http://127.0.0.1:9","bucket_credential":"phone-fixture","#,
                r#""device_key_password":"device-key-password","#,
                r#""device_id":"fedcba9876543210fedcba9876543210","work":"/data/work"}"#
            )
        );
        let secret = joined.trim_end_matches('}').to_string() + r#","recovery":"secret-value"}"#;
        assert!(
            super::assemble_grant_json(&secret, "fedcba9876543210fedcba9876543210", "/data/work")
                .is_err()
        );
    }

    struct TempRoot(PathBuf);

    impl Drop for TempRoot {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[cfg_attr(not(target_os = "ios"), test)]
    pub(super) fn sample_lists_metadata_and_keeps_bytes_out_of_the_json() {
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
        assert!(json.contains("\"name\":\"Publicador\""));
        assert!(json.contains("\"os\":\"exemplo\""));
        assert!(json.contains("\"enrolledUnix\":1700000000"));
        assert!(json.contains("\"lastUnix\":1700000000"));
        assert!(mark_sample(&root.0, "kept.txt", "nope", "", "").is_err());
        mark_sample(&root.0, "kept.txt", "once", "Leitor", "iOS").expect("mark");
        assert_eq!(
            fs::read(root.0.join("work").join("kept.txt")).expect("work"),
            PAYLOAD.as_bytes()
        );
        let marked = sample_catalog(&root.0).expect("marked catalog");
        assert!(marked.contains("\"follow\":\"once\""));
        assert!(marked.contains(READER));
        assert!(marked.contains("\"name\":\"Leitor\""));
        assert!(marked.contains("\"name\":\"Publicador\""));
        assert!(
            !marked.contains(PAYLOAD),
            "marked catalog includes file bytes"
        );
    }

    #[cfg(test)]
    #[cfg_attr(not(target_os = "ios"), test)]
    fn a_sample_killed_halfway_is_rebuilt_on_the_next_open() {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let root = TempRoot(std::env::temp_dir().join(format!("satchel-heal-{nanos}")));
        let first = sample_catalog(&root.0).expect("first");
        assert!(first.contains("kept.txt"));
        // A run killed halfway: the config is there, the rest is not.
        fs::remove_dir_all(root.0.join("repository/snapshots")).expect("break it");
        fs::remove_dir_all(root.0.join("repository/index")).expect("break it");
        let healed = sample_catalog(&root.0).expect("rebuilt");
        assert!(healed.contains("kept.txt"));
    }

    #[test]
    fn empty_device_label_still_marks_the_sample() {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let root = TempRoot(std::env::temp_dir().join(format!("satchel-ffi-empty-{nanos}")));
        let json = sample_catalog(&root.0).expect("catalog");
        assert!(json.contains("\"follow\":\"see\""));
        let root_text = CString::new(root.0.to_str().expect("utf-8 root")).expect("root");
        let path_text = CString::new("kept.txt").expect("path");
        let once_text = CString::new("once").expect("once");
        let nope_text = CString::new("nope").expect("nope");
        let empty = CString::new("").expect("empty");
        let rejected = satchel_mark_sample(
            root_text.as_ptr(),
            path_text.as_ptr(),
            nope_text.as_ptr(),
            empty.as_ptr(),
            empty.as_ptr(),
        );
        assert_eq!(rejected, 1);
        let missing = satchel_mark_sample(
            root_text.as_ptr(),
            path_text.as_ptr(),
            once_text.as_ptr(),
            ptr::null(),
            empty.as_ptr(),
        );
        assert_eq!(missing, 1);
        let untouched = sample_catalog(&root.0).expect("untouched");
        assert!(untouched.contains("\"follow\":\"see\""));
        assert!(!root.0.join("work").join("kept.txt").exists());
        let marked = satchel_mark_sample(
            root_text.as_ptr(),
            path_text.as_ptr(),
            once_text.as_ptr(),
            empty.as_ptr(),
            empty.as_ptr(),
        );
        assert_eq!(marked, 0);
        let bytes = read_sample(&root.0, "kept.txt").expect("read");
        assert_eq!(bytes, PAYLOAD.as_bytes());
        let listed = sample_catalog(&root.0).expect("listed");
        assert!(listed.contains("\"follow\":\"once\""));
        assert!(listed.contains(READER));
        assert!(listed.contains("\"name\":\"Publicador\""));
        assert!(!listed.contains(&format!("\"id\":\"{READER}\",\"name\"")));
        assert!(
            !listed.contains(PAYLOAD),
            "marked catalog includes file bytes"
        );
    }

    #[cfg_attr(not(target_os = "ios"), test)]
    pub(super) fn grant_opens_with_the_device_password() {
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
        mark_grant(&grant_path, "notes/kept.txt", "once", "", "").expect("mark");
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

    #[cfg_attr(not(target_os = "ios"), test)]
    pub(super) fn join_returns_the_grant_without_a_recovery_field() {
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

/// Runs the phone checks on a device. The UI keeps the main thread free.
#[cfg(target_os = "ios")]
#[unsafe(no_mangle)]
pub extern "C" fn satchel_run_phone_checks() -> i32 {
    let checks: [(&str, fn()); 3] = [
        (
            "sample",
            tests::sample_lists_metadata_and_keeps_bytes_out_of_the_json,
        ),
        ("grant", tests::grant_opens_with_the_device_password),
        (
            "join",
            tests::join_returns_the_grant_without_a_recovery_field,
        ),
    ];
    for (name, check) in checks {
        let ran = std::panic::catch_unwind(check);
        if ran.is_err() {
            println!("satchel-device-check {name} failed");
            return 1;
        }
        println!("satchel-device-check {name} ok");
    }
    println!("satchel-device-check passed");
    0
}
