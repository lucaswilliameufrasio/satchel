use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use rand::Rng;

use rustic_backend::BackendOptions;
use rustic_core::repofile::KeyId;
use rustic_core::{
    BackupOptions, ConfigOptions, Credentials, Id, IndexedFullStatus, KeyOptions, OpenStatus,
    PathList, Repository, RepositoryOptions, SnapshotOptions,
};

use crate::Error;

/// Opaque id written into the public fields of a restic key file.
///
/// The value is 32 lowercase hex characters. It is not a hostname.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceId(String);

impl DeviceId {
    /// Build an id from 16 random bytes.
    pub fn generate() -> Self {
        let mut bytes = [0_u8; 16];
        rand::rng().fill_bytes(&mut bytes);
        let mut text = String::with_capacity(32);
        for byte in bytes {
            text.push_str(&format!("{byte:02x}"));
        }
        Self(text)
    }

    /// Accept only the generated shape.
    pub fn parse(value: &str) -> Result<Self, Error> {
        if value.len() != 32 {
            return Err(Error::DeviceId);
        }
        for character in value.chars() {
            if !character.is_ascii_hexdigit() || character.is_ascii_uppercase() {
                return Err(Error::DeviceId);
            }
        }
        Ok(Self(value.to_string()))
    }

    /// Borrow the id text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// S3-compatible location. The secret is only used to build backend options.
pub struct S3Store {
    bucket: String,
    endpoint: String,
    region: String,
    prefix: String,
    access_key_id: String,
    secret_access_key: String,
}

impl S3Store {
    /// Create a store. `prefix` may be empty. The other fields may not.
    pub fn new(
        bucket: impl Into<String>,
        endpoint: impl Into<String>,
        region: impl Into<String>,
        prefix: impl Into<String>,
        access_key_id: impl Into<String>,
        secret_access_key: impl Into<String>,
    ) -> Result<Self, Error> {
        let store = Self {
            bucket: bucket.into(),
            endpoint: endpoint.into(),
            region: region.into(),
            prefix: prefix.into(),
            access_key_id: access_key_id.into(),
            secret_access_key: secret_access_key.into(),
        };
        require_text("bucket", &store.bucket)?;
        require_text("endpoint", &store.endpoint)?;
        require_text("region", &store.region)?;
        require_text("access_key_id", &store.access_key_id)?;
        require_text("secret_access_key", &store.secret_access_key)?;
        Ok(store)
    }
}

/// Where a repository lives.
pub enum Location {
    /// A directory on the local filesystem.
    Directory(PathBuf),
    /// An S3-compatible bucket reached through OpenDAL.
    S3(S3Store),
}

impl Location {
    pub(crate) fn backend_options(&self) -> Result<BackendOptions, Error> {
        match self {
            Self::Directory(path) => {
                let text = path_text(path)?;
                Ok(BackendOptions::default().repository(text))
            }
            Self::S3(store) => {
                let mut options = BTreeMap::new();
                options.insert("bucket".to_string(), store.bucket.clone());
                options.insert("endpoint".to_string(), store.endpoint.clone());
                options.insert("region".to_string(), store.region.clone());
                options.insert("root".to_string(), store.prefix.clone());
                options.insert("access_key_id".to_string(), store.access_key_id.clone());
                options.insert(
                    "secret_access_key".to_string(),
                    store.secret_access_key.clone(),
                );
                options.insert("disable_config_load".to_string(), "true".to_string());
                options.insert("disable_ec2_metadata".to_string(), "true".to_string());
                Ok(BackendOptions::default()
                    .repository("opendal:s3")
                    .options(options))
            }
        }
    }
}

/// Create a new restic repository and write one key for `device_id`.
pub fn init(location: &Location, password: &str, device_id: &DeviceId) -> Result<(), Error> {
    require_password(password)?;
    open_new(location, password, device_id)?;
    Ok(())
}

/// Write a snapshot of `source` and return its id.
pub fn backup_directory(
    location: &Location,
    password: &str,
    device_id: &DeviceId,
    source: &Path,
) -> Result<String, Error> {
    require_password(password)?;
    if !source.is_dir() {
        return Err(Error::SourceDirectory {
            path: source.to_path_buf(),
        });
    }

    let backends = location
        .backend_options()?
        .to_backends()
        .map_err(Error::repository)?;
    let repository = Repository::new(&repository_options(), &backends)
        .map_err(Error::repository)?
        .open(&Credentials::password(password))
        .map_err(Error::repository)?
        .to_indexed_ids()
        .map_err(Error::repository)?;

    let source_text = path_text(source)?;
    let paths = PathList::from_string(&source_text)
        .map_err(Error::repository)?
        .sanitize()
        .map_err(Error::repository)?;
    let snapshot = SnapshotOptions::default()
        .host(Some(device_id.as_str().to_string()))
        .command(Some("satchel".to_string()))
        .to_snapshot()
        .map_err(Error::repository)?;
    let saved = repository
        .backup(&BackupOptions::default(), &paths, snapshot)
        .map_err(Error::repository)?;
    Ok(saved.id.to_hex().as_str().to_string())
}

/// Add a key file for `device_id`, wrapped by `device_password`.
///
/// `opener` is a password that already opens the repository. The new file
/// stores the opaque id, not a machine name. The returned id is 64 hex
/// characters and is not a secret.
pub fn add_device_key(
    location: &Location,
    opener: &str,
    device_id: &DeviceId,
    device_password: &str,
) -> Result<String, Error> {
    require_password(device_password)?;
    let repo = open_unlocked(location, opener)?;
    let options = KeyOptions::default()
        .hostname(Some(device_id.as_str().to_string()))
        .username(Some(device_id.as_str().to_string()))
        .with_created(false);
    let id = repo
        .add_key(device_password, &options)
        .map_err(Error::repository)?;
    Ok(id.into_inner().to_hex().as_str().to_string())
}

/// Delete the key file `key_id`. The key that `opener` used stays in place.
pub fn remove_device_key(location: &Location, opener: &str, key_id: &str) -> Result<(), Error> {
    let id = parse_key_id(key_id)?;
    let repo = open_unlocked(location, opener)?;
    if repo.key_id().as_ref() == Some(&id) {
        return Err(Error::CurrentKey);
    }
    repo.delete_key(&id).map_err(Error::repository)?;
    Ok(())
}

fn open_unlocked(location: &Location, password: &str) -> Result<Repository<OpenStatus>, Error> {
    require_password(password)?;
    let backends = location
        .backend_options()?
        .to_backends()
        .map_err(Error::repository)?;
    Repository::new(&repository_options(), &backends)
        .map_err(Error::repository)?
        .open(&Credentials::password(password))
        .map_err(Error::repository)
}

fn parse_key_id(value: &str) -> Result<KeyId, Error> {
    if value.len() != 64 {
        return Err(Error::KeyId);
    }
    for character in value.chars() {
        if !character.is_ascii_hexdigit() || character.is_ascii_uppercase() {
            return Err(Error::KeyId);
        }
    }
    let inner = Id::from_str(value).map_err(|_| Error::KeyId)?;
    Ok(KeyId::from(inner))
}

fn open_new(location: &Location, password: &str, device_id: &DeviceId) -> Result<(), Error> {
    let backends = location
        .backend_options()?
        .to_backends()
        .map_err(Error::repository)?;
    let key = KeyOptions::default()
        .hostname(Some(device_id.as_str().to_string()))
        .username(Some(device_id.as_str().to_string()))
        .with_created(false);
    let _repository = Repository::new(&repository_options(), &backends)
        .map_err(Error::repository)?
        .init(
            &Credentials::password(password),
            &key,
            &ConfigOptions::default(),
        )
        .map_err(Error::repository)?;
    Ok(())
}

fn repository_options() -> RepositoryOptions {
    RepositoryOptions::default().no_cache(true)
}

pub(crate) fn open_indexed(
    location: &Location,
    password: &str,
) -> Result<Repository<IndexedFullStatus>, Error> {
    require_password(password)?;
    let backends = location
        .backend_options()?
        .to_backends()
        .map_err(Error::repository)?;
    Repository::new(&repository_options(), &backends)
        .map_err(Error::repository)?
        .open(&Credentials::password(password))
        .map_err(Error::repository)?
        .to_indexed()
        .map_err(Error::repository)
}

fn require_password(password: &str) -> Result<(), Error> {
    if password.is_empty() {
        return Err(Error::EmptyPassword);
    }
    Ok(())
}

fn require_text(field: &'static str, value: &str) -> Result<(), Error> {
    if value.is_empty() {
        return Err(Error::EmptyField { field });
    }
    Ok(())
}

fn path_text(path: &Path) -> Result<String, Error> {
    let canonical = fs::canonicalize(path).map_err(Error::repository)?;
    canonical
        .into_os_string()
        .into_string()
        .map_err(|_| Error::repository(format!("path `{}` is not valid unicode", path.display())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_id_rejects_hostname_shaped_text() {
        let parsed = DeviceId::parse("macbook.local");
        assert!(parsed.is_err());
    }

    #[test]
    fn device_id_round_trips_generated_value() {
        let id = DeviceId::generate();
        let parsed = DeviceId::parse(id.as_str()).expect("generated id");
        assert_eq!(parsed, id);
    }

    #[test]
    fn s3_options_carry_the_bucket_and_not_a_local_path() {
        let store = S3Store::new(
            "bucket-a",
            "https://example.test",
            "auto",
            "prefix",
            "key-id",
            "secret-value",
        )
        .expect("store");
        let options = Location::S3(store).backend_options().expect("options");
        assert_eq!(options.repository.as_deref(), Some("opendal:s3"));
        assert_eq!(
            options.options.get("bucket").map(String::as_str),
            Some("bucket-a")
        );
        assert_eq!(
            options.options.get("secret_access_key").map(String::as_str),
            Some("secret-value")
        );
        assert_eq!(
            options.options.get("root").map(String::as_str),
            Some("prefix")
        );
        assert_eq!(
            options
                .options
                .get("disable_config_load")
                .map(String::as_str),
            Some("true")
        );
        assert_eq!(
            options
                .options
                .get("disable_ec2_metadata")
                .map(String::as_str),
            Some("true")
        );
    }
}
