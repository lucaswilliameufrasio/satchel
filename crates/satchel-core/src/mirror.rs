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
//!
//! Files move between the folder and the work copy file to file, by the
//! platform's own streams, so a round never holds a whole file in memory. The
//! client says which work files the sync wrote, so the round does not read
//! the work copy to find out.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::Error;
use crate::format::{Location, snapshot_ids};
use crate::sync::{
    Client, Follow, SyncOutcome, copy_file_into, is_marked, normalize_path, regular_files,
    remove_work, temp_beside, work_path,
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
    /// Copy one folder file to `dest`, a new file the round owns.
    fn copy_in(&mut self, path: &str, dest: &Path) -> Result<(), Error>;
    /// Create or replace one folder file with the bytes of `source`, creating parent folders.
    fn copy_out(&mut self, source: &Path, path: &str) -> Result<(), Error>;
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

    fn copy_in(&mut self, path: &str, dest: &Path) -> Result<(), Error> {
        fs::copy(work_path(&self.root, path)?, dest).map_err(Error::repository)?;
        Ok(())
    }

    fn copy_out(&mut self, source: &Path, path: &str) -> Result<(), Error> {
        copy_file_into(source, &self.root, path)
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
    /// True when the round found nothing new and did not open the repository.
    pub quiet: bool,
    /// What the sync itself wrote. Absent on a quiet round.
    pub outcome: Option<SyncOutcome>,
}

const UNSEEN: (u64, i64) = (u64::MAX, i64::MIN);

#[derive(Default, Deserialize, Serialize)]
struct Manifest {
    files: BTreeMap<String, (u64, i64)>,
    /// Snapshot ids in the repository at the end of the last round.
    #[serde(default)]
    snapshots: Option<Vec<String>>,
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

/// Run one round: folder into the work copy, sync, work copy back to the folder.
///
/// Every folder file is followed as [`Follow::Always`]. Catalog marks the client
/// already carries (for example a file marked to download once) still apply, and
/// what they download lands in the folder. `manifest` is a file the app owns,
/// outside the folder.
///
/// When no follow was set on `client`, the folder matches the manifest and the
/// snapshot ids match the last round, the round stops there: one listing of
/// `snapshots/`, no password and no key derivation. A changed device name waits
/// for the next round that has something to do.
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
    let ids = snapshot_ids(location)?;
    if !client.has_marks() && manifest.snapshots.as_ref() == Some(&ids) && manifest.files == before
    {
        return Ok(MirrorReport {
            imported: Vec::new(),
            deleted_here: Vec::new(),
            exported: Vec::new(),
            deleted_there: Vec::new(),
            deferred: Vec::new(),
            quiet: true,
            outcome: None,
        });
    }
    let work = client.work().to_path_buf();

    let mut imported = Vec::new();
    for (path, seen) in &before {
        if manifest.files.get(path) == Some(seen) {
            continue;
        }
        let target = work_path(&work, path)?;
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(Error::repository)?;
        }
        let temp = temp_beside(&target)?;
        if let Err(err) = tree.copy_in(path, &temp) {
            let _ = fs::remove_file(&temp);
            return Err(err);
        }
        fs::rename(&temp, &target).map_err(Error::repository)?;
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

    client.take_touched();
    let outcome = client.sync(location)?;
    let touched = client.take_touched();
    let post: BTreeSet<String> = regular_files(&work)?.into_iter().collect();

    // What the sync wrote, and any work file the folder does not have yet.
    let mut copy_out = Vec::new();
    for path in &post {
        if touched.contains(path) || !before.contains_key(path) {
            copy_out.push(path.clone());
        }
    }
    let mut gone = Vec::new();
    for path in &touched {
        if !post.contains(path) && before.contains_key(path) {
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
        tree.copy_out(&work_path(&work, &path)?, &path)?;
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
    // Ids after this round's own snapshot, so the next quiet check matches.
    manifest.snapshots = if deferred.is_empty() {
        Some(snapshot_ids(location)?)
    } else {
        None
    };
    manifest.files = next;
    save_manifest(manifest_path, &manifest)?;

    Ok(MirrorReport {
        imported,
        deleted_here,
        exported,
        deleted_there,
        deferred: deferred.into_iter().collect(),
        quiet: false,
        outcome: Some(outcome),
    })
}
