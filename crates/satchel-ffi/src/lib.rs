//! JNI catalog for Android.
//!
//! `sampleCatalog` builds a private repository and returns metadata only.
//! File bytes are available from `readFile` after a mark asks for them.

use std::fs;
use std::path::Path;

use jni::EnvUnowned;
use jni::objects::{JClass, JString};
use jni::sys::{jbyteArray, jstring};
use satchel_core::{Client, DeviceId, Follow, Location, cursors, init, read_version};
use serde::Serialize;

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
    let mut rows = Vec::new();
    for entry in &entries {
        rows.push(RowJson {
            path: &entry.path,
            directory: entry.is_directory,
            size: entry.size,
            version_unix: entry.version_unix,
            devices: &entry.devices,
            follow: entry.follow,
        });
    }
    let json = serde_json::to_string(&rows).map_err(|_| CatalogFailure)?;
    if json.contains(PAYLOAD) {
        return Err(CatalogFailure);
    }
    Ok(json)
}

fn read_sample(root: &Path, path: &str) -> Result<Vec<u8>, CatalogFailure> {
    let location = ensure_sample(root)?;
    let listed = cursors(&location, PASSWORD).map_err(|_| CatalogFailure)?;
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
    let Some(cursor) = chosen else {
        return Err(CatalogFailure);
    };
    read_version(&location, PASSWORD, &cursor.snapshot_id, path).map_err(|_| CatalogFailure)
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

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{PAYLOAD, read_sample, sample_catalog};

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
    }
}
