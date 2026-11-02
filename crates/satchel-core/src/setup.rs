//! Create a repository on the storage the person chose.
//!
//! The same fields describe a folder, S3, R2, and B2 on every device. The gate
//! runs before the first write, so a refused bucket stays empty.

use std::path::PathBuf;

use rand::Rng;

use crate::format::{Location, S3Store, init};
use crate::probe::probe_object_protection;
use crate::storage::{SetupCheck, StorageKind, check_setup};
use crate::{DeviceId, Error};

/// Where the new repository goes and, for object storage, how to reach it.
#[derive(Clone)]
pub struct StorageSetup {
    /// Folder, S3, R2, or B2. WebDAV and SFTP have no setup in this cut.
    pub kind: StorageKind,
    /// Repository directory. Used only when `kind` is a folder.
    pub directory: PathBuf,
    /// Origin such as `https://s3.us-east-1.amazonaws.com`.
    pub endpoint: String,
    /// Bucket name.
    pub bucket: String,
    /// Signing region. R2 uses `auto`.
    pub region: String,
    /// Repository root inside the bucket. May be empty.
    pub prefix: String,
    /// Maintenance access key id. It can delete packs, indexes, snapshots, and keys.
    pub access_key_id: String,
    /// Maintenance secret.
    pub secret_access_key: String,
}

impl std::fmt::Debug for StorageSetup {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("StorageSetup")
            .field("kind", &self.kind)
            .field("bucket", &self.bucket)
            .finish_non_exhaustive()
    }
}

impl StorageSetup {
    /// Folder repository.
    pub fn directory(path: impl Into<PathBuf>) -> Self {
        Self {
            kind: StorageKind::Directory,
            directory: path.into(),
            endpoint: String::new(),
            bucket: String::new(),
            region: String::new(),
            prefix: String::new(),
            access_key_id: String::new(),
            secret_access_key: String::new(),
        }
    }

    /// True for S3, R2, and B2.
    pub fn is_object_store(&self) -> bool {
        matches!(
            self.kind,
            StorageKind::S3 | StorageKind::R2 | StorageKind::B2
        )
    }

    /// Where the repository lives. Object fields are checked here.
    pub fn location(&self) -> Result<Location, Error> {
        match self.kind {
            StorageKind::Directory => {
                if self.directory.as_os_str().is_empty() {
                    return Err(Error::EmptyField { field: "directory" });
                }
                Ok(Location::Directory(self.directory.clone()))
            }
            StorageKind::S3 | StorageKind::R2 | StorageKind::B2 => {
                if !self.endpoint.starts_with("http://") && !self.endpoint.starts_with("https://") {
                    return Err(Error::EmptyField { field: "endpoint" });
                }
                let store = S3Store::new(
                    self.bucket.clone(),
                    self.endpoint.clone(),
                    self.region.clone(),
                    self.prefix.clone(),
                    self.access_key_id.clone(),
                    self.secret_access_key.clone(),
                )?;
                Ok(Location::S3(store))
            }
            StorageKind::WebDav | StorageKind::Sftp => Err(Error::UnsupportedStorage),
        }
    }
}

/// What a successful creation leaves for the person to keep.
pub struct CreatedRepository {
    /// Opens the repository with the official `restic`. The caller stores it once.
    pub recovery_password: String,
    /// `Ready` for object storage. `Unprotected` for a folder.
    pub protection: SetupCheck,
}

impl std::fmt::Debug for CreatedRepository {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CreatedRepository")
            .field("protection", &self.protection)
            .finish_non_exhaustive()
    }
}

/// Random recovery password: 32 bytes as lowercase hex.
pub fn generate_recovery_password() -> String {
    let mut bytes = [0_u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    let mut text = String::with_capacity(64);
    for byte in bytes {
        text.push_str(&format!("{byte:02x}"));
    }
    text
}

/// Gate the storage, then create the repository with `password` and one key for `device_id`.
///
/// S3, R2, and B2 are probed first. Versioning off or a lifecycle rule that
/// deletes objects refuses the setup before anything is written. A location
/// that already has a `config` object is refused so a second device cannot
/// overwrite someone else's repository.
pub fn create_repository(
    setup: &StorageSetup,
    device_id: &DeviceId,
    password: &str,
) -> Result<CreatedRepository, Error> {
    let location = setup.location()?;
    let protection = check_storage(setup)?;
    if location.config_present()? {
        return Err(Error::RepositoryExists);
    }
    if setup.kind == StorageKind::Directory {
        std::fs::create_dir_all(&setup.directory).map_err(Error::repository)?;
    }
    init(&location, password, device_id)?;
    Ok(CreatedRepository {
        recovery_password: password.to_string(),
        protection,
    })
}

/// Run the setup gate for `setup` without writing.
pub fn check_storage(setup: &StorageSetup) -> Result<SetupCheck, Error> {
    if !setup.is_object_store() {
        return check_setup(setup.kind, None, false);
    }
    setup.location()?;
    let probed = probe_object_protection(
        &setup.endpoint,
        &setup.bucket,
        &setup.region,
        &setup.access_key_id,
        &setup.secret_access_key,
    )?;
    check_setup(
        setup.kind,
        probed.versioning,
        probed.lifecycle_deletes_objects,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folder_setup_creates_once_and_warns() {
        let root = std::env::temp_dir().join(format!("satchel-setup-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let setup = StorageSetup::directory(root.join("repo"));
        let device = DeviceId::generate();
        let password = generate_recovery_password();
        assert_eq!(password.len(), 64);
        let made = create_repository(&setup, &device, &password).expect("create");
        assert_eq!(made.protection, SetupCheck::Unprotected);
        assert!(root.join("repo/config").is_file());
        let again = create_repository(&setup, &device, &password);
        assert!(matches!(again, Err(Error::RepositoryExists)));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn object_fields_are_checked_before_any_network_call() {
        let mut setup = StorageSetup::directory("/tmp/unused");
        setup.kind = StorageKind::B2;
        assert!(setup.location().is_err());
        setup.endpoint = "ftp://nope".into();
        setup.bucket = "b".into();
        setup.region = "r".into();
        setup.access_key_id = "k".into();
        setup.secret_access_key = "s".into();
        assert!(setup.location().is_err());
        setup.kind = StorageKind::WebDav;
        assert!(matches!(setup.location(), Err(Error::UnsupportedStorage)));
    }
}
