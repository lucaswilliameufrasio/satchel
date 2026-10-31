use std::fs;
use std::path::{Path, PathBuf};

use satchel_core::DeviceId;
use serde::{Deserialize, Serialize};

use crate::Error;

/// One folder and one repository. Secrets stay in the keystore.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Profile {
    /// Directory the person syncs.
    pub folder: PathBuf,
    /// Local restic repository. Not inside `folder`.
    pub repository: PathBuf,
    /// Opaque device id.
    pub device_id: DeviceId,
    /// S3-compatible bucket. Absent when the repository is only the local directory.
    pub object_store: Option<ObjectStore>,
}

/// Bucket coordinates. The access key stays in the keystore.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObjectStore {
    /// Bucket name.
    pub bucket: String,
    /// Origin such as `http://127.0.0.1:4566`. No path and no credential.
    pub endpoint: String,
    /// Signing region.
    pub region: String,
    /// Repository root inside the bucket. May be empty.
    pub prefix: String,
}

#[derive(Serialize, Deserialize)]
struct ProfileDoc {
    folder: String,
    repository: String,
    device_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    object_store: Option<ObjectStoreDoc>,
}

#[derive(Clone, Serialize, Deserialize)]
struct ObjectStoreDoc {
    bucket: String,
    endpoint: String,
    region: String,
    prefix: String,
}

pub(crate) fn load(config_dir: &Path) -> Result<Option<Profile>, Error> {
    let path = profile_path(config_dir);
    if !path.exists() {
        return Ok(None);
    }
    let bytes = fs::read(&path).map_err(|_| Error::Io)?;
    let doc: ProfileDoc = serde_json::from_slice(&bytes).map_err(|_| Error::Profile)?;
    let folder = PathBuf::from(doc.folder);
    let repository = PathBuf::from(doc.repository);
    let device_id = DeviceId::parse(&doc.device_id)?;
    let object_store = match doc.object_store {
        None => None,
        Some(store) => Some(parse_object_store(store)?),
    };
    Ok(Some(Profile {
        folder,
        repository,
        device_id,
        object_store,
    }))
}

pub(crate) fn create(
    config_dir: &Path,
    folder: PathBuf,
    repository: PathBuf,
    device_id: DeviceId,
) -> Result<Profile, Error> {
    fs::create_dir_all(config_dir).map_err(|_| Error::Io)?;
    if profile_path(config_dir).exists() {
        return Err(Error::ProfileExists);
    }
    validate(config_dir, &folder, &repository)?;
    let profile = Profile {
        folder,
        repository,
        device_id,
        object_store: None,
    };
    write_profile(config_dir, &profile)?;
    Ok(profile)
}

pub(crate) fn attach_object_store(
    config_dir: &Path,
    profile: &Profile,
    object_store: ObjectStore,
) -> Result<Profile, Error> {
    validate_object_store(&object_store)?;
    let mut updated = profile.clone();
    updated.object_store = Some(object_store);
    write_profile(config_dir, &updated)?;
    Ok(updated)
}

fn write_profile(config_dir: &Path, profile: &Profile) -> Result<(), Error> {
    let doc = ProfileDoc {
        folder: path_text(&profile.folder)?,
        repository: path_text(&profile.repository)?,
        device_id: profile.device_id.as_str().to_string(),
        object_store: profile.object_store.as_ref().map(|store| ObjectStoreDoc {
            bucket: store.bucket.clone(),
            endpoint: store.endpoint.clone(),
            region: store.region.clone(),
            prefix: store.prefix.clone(),
        }),
    };
    let bytes = serde_json::to_vec_pretty(&doc).map_err(|_| Error::Profile)?;
    let temporary = config_dir.join("profile.json.tmp");
    fs::write(&temporary, &bytes).map_err(|_| Error::Io)?;
    fs::rename(&temporary, profile_path(config_dir)).map_err(|_| Error::Io)?;
    Ok(())
}

fn parse_object_store(doc: ObjectStoreDoc) -> Result<ObjectStore, Error> {
    let store = ObjectStore {
        bucket: doc.bucket,
        endpoint: doc.endpoint,
        region: doc.region,
        prefix: doc.prefix,
    };
    validate_object_store(&store)?;
    Ok(store)
}

fn validate_object_store(store: &ObjectStore) -> Result<(), Error> {
    if store.bucket.is_empty() || store.region.is_empty() {
        return Err(Error::Profile);
    }
    if !store.endpoint.starts_with("http://") && !store.endpoint.starts_with("https://") {
        return Err(Error::Profile);
    }
    if store.endpoint.contains('@')
        || store.bucket.contains('/')
        || store.prefix.starts_with('/')
        || store.prefix.ends_with('/')
    {
        return Err(Error::Profile);
    }
    Ok(())
}

fn validate(config_dir: &Path, folder: &Path, repository: &Path) -> Result<(), Error> {
    if !folder.is_absolute() {
        return Err(Error::Path {
            path: folder.to_path_buf(),
        });
    }
    if !repository.is_absolute() {
        return Err(Error::Path {
            path: repository.to_path_buf(),
        });
    }
    if folder == repository || repository.starts_with(folder) {
        return Err(Error::Path {
            path: repository.to_path_buf(),
        });
    }
    if config_dir.starts_with(folder) {
        return Err(Error::Path {
            path: config_dir.to_path_buf(),
        });
    }
    let metadata = fs::metadata(folder).map_err(|_| Error::Path {
        path: folder.to_path_buf(),
    })?;
    if !metadata.is_dir() {
        return Err(Error::Path {
            path: folder.to_path_buf(),
        });
    }
    Ok(())
}

fn path_text(path: &Path) -> Result<String, Error> {
    path.to_str()
        .map(str::to_string)
        .ok_or_else(|| Error::Path {
            path: path.to_path_buf(),
        })
}

pub(crate) fn profile_path(config_dir: &Path) -> PathBuf {
    config_dir.join("profile.json")
}
