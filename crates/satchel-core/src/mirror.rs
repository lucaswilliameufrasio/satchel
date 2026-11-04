//! A folder the phone cannot hand to the client as a plain path.
//!
//! Android gives an app a folder through the Storage Access Framework, which
//! reads and writes by URI, so the client keeps syncing its private work copy and
//! one round copies between that copy and the folder. The iOS folder picker gives
//! a path, and [`DirectoryTree`] serves it with the same round.
//!
//! A manifest remembers the size and modification time of every folder file at
//! the end of the last round. A folder file that differs from it was changed by
//! the person and is copied in. A manifest file missing from the folder was
//! deleted by the person and is deleted in the work copy. A work file that the
//! sync changed or added is copied out, unless the person changed that same
//! folder file during the round; then it waits for the next round.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::Error;
use crate::format::Location;
use crate::sync::{
    Client, Follow, SyncOutcome, is_marked, normalize_path, read_work, regular_files, remove_work,
    work_path, write_under,
};

/// One file in the folder. `path` is relative and uses `/`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TreeFile {
    /// Relative path using `/`.
    pub path: String,
    /// Size in bytes.
    pub size: u64,
    /// Modification time in milliseconds since the Unix epoch, as the folder reports it.
    pub modified: i64,
}

/// Access to the folder the person picked.
///
/// `list` must fail when the folder cannot be read. Returning an empty list
/// instead would look like the person deleted every file.
pub trait FolderTree {
    /// Every regular file under the folder.
    fn list(&mut self) -> Result<Vec<TreeFile>, Error>;
    /// The bytes of one file.
    fn read(&mut self, path: &str) -> Result<Vec<u8>, Error>;
    /// Create or replace one file, creating parent folders.
    fn write(&mut self, path: &str, bytes: &[u8]) -> Result<(), Error>;
    /// Delete one file. A missing file is not an error.
    fn remove(&mut self, path: &str) -> Result<(), Error>;
}

/// A folder the app can reach by path.
pub struct DirectoryTree {
    root: PathBuf,
}

impl DirectoryTree {
    /// Serve `root`, which must exist.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
}

impl FolderTree for DirectoryTree {
    fn list(&mut self) -> Result<Vec<TreeFile>, Error> {
        if !self.root.is_dir() {
            return Err(Error::FolderUnavailable);
        }
        let mut files = Vec::new();
        for path in regular_files(&self.root)? {
            let metadata =
                fs::symlink_metadata(work_path(&self.root, &path)?).map_err(Error::repository)?;
            let modified = metadata
                .modified()
                .ok()
                .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|time| i64::try_from(time.as_millis()).unwrap_or(i64::MAX))
                .unwrap_or(0);
            files.push(TreeFile {
                path,
                size: metadata.len(),
                modified,
            });
        }
        Ok(files)
    }

    fn read(&mut self, path: &str) -> Result<Vec<u8>, Error> {
        read_work(&self.root, path)?.ok_or_else(|| Error::repository("folder file is gone"))
    }

    fn write(&mut self, path: &str, bytes: &[u8]) -> Result<(), Error> {
        write_under(&self.root, path, bytes)
    }

    fn remove(&mut self, path: &str) -> Result<(), Error> {
        remove_work(&self.root, path)
    }
}

/// What one round moved.
#[derive(Debug)]
pub struct MirrorReport {
    /// Folder files copied into the work copy.
    pub imported: Vec<String>,
    /// Work files deleted because the person deleted them in the folder.
    pub deleted_here: Vec<String>,
    /// Work files copied out to the folder.
    pub exported: Vec<String>,
    /// Folder files deleted because the sync deleted them.
    pub deleted_there: Vec<String>,
    /// Paths the person changed during the round. The next round takes them.
    pub deferred: Vec<String>,
    /// What the sync itself wrote.
    pub outcome: SyncOutcome,
}

const UNSEEN: (u64, i64) = (u64::MAX, i64::MIN);

#[derive(Default, Deserialize, Serialize)]
struct Manifest {
    files: BTreeMap<String, (u64, i64)>,
}

fn load_manifest(path: &Path) -> Result<Manifest, Error> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(Error::repository),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(Manifest::default()),
        Err(err) => Err(Error::repository(err)),
    }
}

fn save_manifest(path: &Path, manifest: &Manifest) -> Result<(), Error> {
    let bytes = serde_json::to_vec(manifest).map_err(Error::repository)?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| Error::repository("manifest name is not valid unicode"))?;
    let temp = path.with_file_name(format!(".{name}.tmp"));
    fs::write(&temp, bytes).map_err(Error::repository)?;
    fs::rename(&temp, path).map_err(Error::repository)
}

/// Listing keyed by path. Unsafe paths, conflict copies and temporary files are dropped.
fn listing(files: Vec<TreeFile>) -> BTreeMap<String, (u64, i64)> {
    let mut map = BTreeMap::new();
    for file in files {
        let Ok(path) = normalize_path(&file.path) else {
            continue;
        };
        if path != file.path || is_marked(&path) {
            continue;
        }
        let name = path.rsplit('/').next().unwrap_or(&path);
        if name.ends_with(".satchel-read") || name.ends_with(".satchel-write") {
            continue;
        }
        map.insert(path, (file.size, file.modified));
    }
    map
}

fn digests(work: &Path) -> Result<BTreeMap<String, [u8; 32]>, Error> {
    let mut map = BTreeMap::new();
    for path in regular_files(work)? {
        if let Some(bytes) = read_work(work, &path)? {
            map.insert(path, Sha256::digest(&bytes).into());
        }
    }
    Ok(map)
}

/// Run one round: folder into the work copy, sync, work copy back to the folder.
///
/// Every folder file is followed as [`Follow::Always`]. Catalog marks the client
/// already carries (for example a file marked to download once) still apply, and
/// what they download lands in the folder. `manifest` is a file the app owns,
/// outside the folder.
pub fn sync_mirrored(
    client: &mut Client,
    location: &Location,
    tree: &mut dyn FolderTree,
    manifest_path: &Path,
) -> Result<MirrorReport, Error> {
    let mut manifest = load_manifest(manifest_path)?;
    let before = listing(tree.list()?);
    if before.is_empty() && !manifest.files.is_empty() {
        return Err(Error::FolderUnavailable);
    }
    let work = client.work().to_path_buf();

    let mut imported = Vec::new();
    for (path, seen) in &before {
        if manifest.files.get(path) == Some(seen) {
            continue;
        }
        let bytes = tree.read(path)?;
        write_under(&work, path, &bytes)?;
        imported.push(path.clone());
    }
    let mut deleted_here = Vec::new();
    for path in manifest.files.keys() {
        if before.contains_key(path) {
            continue;
        }
        remove_work(&work, path)?;
        deleted_here.push(path.clone());
    }
    for path in before.keys() {
        client.follow(path, Follow::Always)?;
    }

    let pre = digests(&work)?;
    let outcome = client.sync(location)?;
    let post = digests(&work)?;

    let mut copy_out = Vec::new();
    for (path, digest) in &post {
        if pre.get(path) != Some(digest) || !before.contains_key(path) {
            copy_out.push(path.clone());
        }
    }
    let mut gone = Vec::new();
    for path in pre.keys() {
        if !post.contains_key(path) && before.contains_key(path) {
            gone.push(path.clone());
        }
    }

    let during = listing(tree.list()?);
    let mut deferred = BTreeSet::new();
    let mut exported = Vec::new();
    for path in copy_out {
        if during.get(&path) != before.get(&path) {
            deferred.insert(path);
            continue;
        }
        let Some(bytes) = read_work(&work, &path)? else {
            continue;
        };
        tree.write(&path, &bytes)?;
        exported.push(path);
    }
    let mut deleted_there = Vec::new();
    for path in gone {
        if during.get(&path) != before.get(&path) {
            deferred.insert(path);
            continue;
        }
        tree.remove(&path)?;
        deleted_there.push(path);
    }
    for (path, seen) in &during {
        if before.get(path) != Some(seen) && !exported.contains(path) {
            deferred.insert(path.clone());
        }
    }

    let after = listing(tree.list()?);
    let mut next = BTreeMap::new();
    for (path, seen) in &after {
        if !deferred.contains(path) {
            next.insert(path.clone(), *seen);
        }
    }
    for path in &deferred {
        // An entry that cannot match a listing: next round copies the file in if
        // it is still there, and deletes the work copy if the person deleted it.
        let old = manifest.files.get(path).copied().unwrap_or(UNSEEN);
        next.insert(path.clone(), old);
    }
    manifest.files = next;
    save_manifest(manifest_path, &manifest)?;

    Ok(MirrorReport {
        imported,
        deleted_here,
        exported,
        deleted_there,
        deferred: deferred.into_iter().collect(),
        outcome,
    })
}
