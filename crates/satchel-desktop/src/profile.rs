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
    /// Restic repository path or a note of where it lives. Not inside `folder`.
    pub repository: PathBuf,
    /// Opaque device id.
    pub device_id: DeviceId,
}

#[derive(Serialize, Deserialize)]
struct ProfileDoc {
    folder: String,
    repository: String,
    device_id: String,
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
    Ok(Some(Profile {
        folder,
        repository,
        device_id,
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
    let doc = ProfileDoc {
        folder: path_text(&folder)?,
        repository: path_text(&repository)?,
        device_id: device_id.as_str().to_string(),
    };
    let bytes = serde_json::to_vec_pretty(&doc).map_err(|_| Error::Profile)?;
    let temporary = config_dir.join("profile.json.tmp");
    fs::write(&temporary, &bytes).map_err(|_| Error::Io)?;
    fs::rename(&temporary, profile_path(config_dir)).map_err(|_| Error::Io)?;
    Ok(Profile {
        folder,
        repository,
        device_id,
    })
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
