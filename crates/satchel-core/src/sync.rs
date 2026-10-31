//! Headless sync of regular files through a restic repository.
//!
//! The cursor is the snapshot parent plus the opaque device id. Subscriptions,
//! tombstones, and conflict marks ride in the encrypted snapshot description.
//! File bytes are read only for Baixar agora and Sincronizar sempre.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use rustic_core::jiff::{Timestamp, tz::TimeZone};
use rustic_core::repofile::SnapshotFile;
use rustic_core::{
    BackupOptions, IndexedFullStatus, LsOptions, ParentOptions, PathList, Repository,
    SnapshotGroupCriterion, SnapshotOptions, TreeId,
};

use crate::Error;
use crate::format::{DeviceId, Location, init, open_indexed};

const STATE_VERSION: u32 = 1;
const CONFLICT_MARK: &str = ".satchel-conflict-";
/// Virtual root stored in the snapshot so the tree does not contain a machine path.
const TREE_ROOT: &str = "satchel";

/// How this device treats a file or a folder.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Follow {
    /// Metadata only. The content stays in storage.
    See,
    /// Download once and do not follow later changes.
    Once,
    /// Download, upload, and follow later changes.
    Always,
}

/// Snapshot cursor for one device.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Cursor {
    /// Id of the snapshot that recorded this cursor.
    pub snapshot_id: String,
    /// Parent snapshot this one is based on.
    pub parent_id: Option<String>,
    /// Opaque device id stored as the snapshot host.
    pub device_id: String,
    /// Controlled clock, as seconds since the Unix epoch.
    pub time_unix: i64,
}

/// One catalog row. Content packs are not read to build it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CatalogEntry {
    /// Relative path using `/`.
    pub path: String,
    /// True when the entry is a directory.
    pub is_directory: bool,
    /// Size in bytes. Directories are zero.
    pub size: u64,
    /// Unix time of the snapshot that won this path.
    pub version_unix: i64,
    /// Devices whose published state keeps a copy.
    pub devices: Vec<String>,
    /// This device's follow state for the path.
    pub follow: Follow,
}

/// A conflicting version stored beside the primary path.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MarkedConflict {
    /// Primary path that was left in place.
    pub path: String,
    /// Marked path that holds the other bytes.
    pub marked_path: String,
    /// Device that uploaded the marked version.
    pub device_id: String,
    /// Snapshot that contains the marked bytes.
    pub snapshot_id: String,
}

/// What one sync wrote.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyncOutcome {
    /// Cursor after the sync. Absent when this device has not published yet.
    pub cursor: Option<Cursor>,
    /// Conflicts created by this call.
    pub conflicts: Vec<MarkedConflict>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct StateDoc {
    satchel: u32,
    #[serde(default)]
    follows: BTreeMap<String, Follow>,
    #[serde(default)]
    tombstones: BTreeSet<String>,
    #[serde(default)]
    once: BTreeMap<String, String>,
    #[serde(default)]
    conflicts: Vec<StoredConflict>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct StoredConflict {
    path: String,
    marked_path: String,
    device_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    snapshot_id: Option<String>,
}

#[derive(Clone, Debug)]
struct Snap {
    id: String,
    parent_id: Option<String>,
    device_id: String,
    time_unix: i64,
    tree: TreeId,
    state: StateDoc,
    files: BTreeMap<String, FileMeta>,
}

#[derive(Clone, Copy, Debug)]
struct FileMeta {
    size: u64,
    is_dir: bool,
}

#[derive(Clone, Debug)]
enum Presence {
    Bytes(Vec<u8>),
    Deleted,
}

#[derive(Clone, Debug)]
struct Base {
    presence: Presence,
}

enum Winner {
    Present {
        snapshot_id: String,
        time_unix: i64,
        size: u64,
    },
    Deleted {
        snapshot_id: String,
        time_unix: i64,
    },
}

enum Plan {
    Adopt {
        bytes: Option<Vec<u8>>,
    },
    Conflict {
        local: Vec<u8>,
        bytes: Option<Vec<u8>>,
    },
    Upload {
        bytes: Option<Vec<u8>>,
    },
    Restore(Vec<u8>),
}

/// One headless device. The work directory is the local copy.
pub struct Client {
    device_id: DeviceId,
    work: PathBuf,
    password: String,
    clock: i64,
    follows: BTreeMap<String, Follow>,
    cursor: Option<String>,
    last_cursor: Option<Cursor>,
    bases: BTreeMap<String, Base>,
    once_done: BTreeMap<String, String>,
    conflicts: Vec<StoredConflict>,
    tombstones: BTreeSet<String>,
    published: Option<StateDoc>,
}

impl Client {
    /// Open a device against `work`. `password` is any key that opens the repository.
    pub fn open(
        device_id: DeviceId,
        work: impl Into<PathBuf>,
        password: impl Into<String>,
    ) -> Result<Self, Error> {
        let password = password.into();
        if password.is_empty() {
            return Err(Error::EmptyPassword);
        }
        let work = work.into();
        fs::create_dir_all(&work).map_err(Error::repository)?;
        Ok(Self {
            device_id,
            work,
            password,
            clock: 0,
            follows: BTreeMap::new(),
            cursor: None,
            last_cursor: None,
            bases: BTreeMap::new(),
            once_done: BTreeMap::new(),
            conflicts: Vec::new(),
            tombstones: BTreeSet::new(),
            published: None,
        })
    }

    /// Set the controlled clock used as the snapshot time.
    pub fn set_clock(&mut self, unix_seconds: i64) -> Result<(), Error> {
        if unix_seconds < 0 {
            return Err(Error::Clock);
        }
        self.clock = unix_seconds;
        Ok(())
    }

    /// Set how this device treats `path` and, for a folder, its children.
    pub fn follow(&mut self, path: &str, follow: Follow) -> Result<(), Error> {
        let path = normalize_path(path)?;
        self.follows.insert(path, follow);
        Ok(())
    }

    /// Borrow the opaque device id.
    pub fn device_id(&self) -> &DeviceId {
        &self.device_id
    }

    /// List names, sizes, times, and devices without writing the work copy.
    pub fn catalog(&self, location: &Location) -> Result<Vec<CatalogEntry>, Error> {
        let repo = open_indexed(location, &self.password)?;
        let snaps = load_snaps(&repo)?;
        let latest = latest_by_device(&snaps);
        let follows = merged_follows(&self.follows, our_snap(&latest, self.device_id.as_str()));
        let found = winners(&latest);
        let mut entries = Vec::new();
        let mut paths = BTreeSet::new();
        for snap in latest.values() {
            for path in snap.files.keys() {
                if !is_marked(path) {
                    paths.insert(path.clone());
                }
            }
        }
        for path in paths {
            // A new phone has no marks yet. The row is still metadata, as Ver.
            let follow = follow_for(&follows, &path).unwrap_or(Follow::See);
            let Some(Winner::Present {
                time_unix, size, ..
            }) = found.get(&path)
            else {
                continue;
            };
            let is_directory = latest
                .values()
                .any(|snap| snap.files.get(&path).is_some_and(|meta| meta.is_dir));
            entries.push(CatalogEntry {
                devices: devices_keeping(&latest, &path),
                path,
                is_directory,
                size: *size,
                version_unix: *time_unix,
                follow,
            });
        }
        Ok(entries)
    }

    /// Exchange metadata and, where the follow state says so, file bytes.
    pub fn sync(&mut self, location: &Location) -> Result<SyncOutcome, Error> {
        let repo = open_indexed(location, &self.password)?;
        let snaps = load_snaps(&repo)?;
        self.adopt_clock(&snaps);
        let materialize = if self.cursor.is_none() {
            self.resume_from_repository(&repo, &snaps)?
        } else {
            false
        };
        let latest = latest_by_device(&snaps);
        let follows = merged_follows(&self.follows, our_snap(&latest, self.device_id.as_str()));
        for (path, follow) in &follows {
            self.follows.entry(path.clone()).or_insert(*follow);
        }
        let found = winners(&latest);
        let local = scan_followed(&self.work, &self.follows, Follow::Always)?;
        let plans = self.plan_always(&repo, &latest, &found, &local, materialize)?;
        let mut dirty = false;
        let mut new_marks = Vec::new();
        for (path, plan) in plans {
            match plan {
                Plan::Adopt { bytes } => {
                    self.apply_remote(&path, bytes)?;
                }
                Plan::Conflict { local, bytes } => {
                    let marked_path = format!(
                        "{path}{CONFLICT_MARK}{}-{}",
                        self.device_id.as_str(),
                        self.clock
                    );
                    new_marks.push((
                        StoredConflict {
                            path: path.clone(),
                            marked_path,
                            device_id: self.device_id.as_str().to_string(),
                            snapshot_id: None,
                        },
                        local,
                    ));
                    self.apply_remote(&path, bytes)?;
                }
                Plan::Upload { bytes } => {
                    dirty = true;
                    if bytes.is_some() {
                        self.tombstones.remove(&path);
                    } else {
                        remove_work(&self.work, &path)?;
                        self.tombstones.insert(path);
                    }
                }
                Plan::Restore(bytes) => {
                    write_work(&self.work, &path, &bytes)?;
                }
            }
        }
        self.apply_once(&repo, &snaps, &found)?;
        self.publish(&repo, &latest, dirty, new_marks)
    }

    /// Conflicts recorded on the latest snapshot of every device.
    pub fn conflicts(&self, location: &Location) -> Result<Vec<MarkedConflict>, Error> {
        let repo = open_indexed(location, &self.password)?;
        let snaps = load_snaps(&repo)?;
        Ok(collect_conflicts(&latest_by_device(&snaps)))
    }

    fn adopt_clock(&mut self, snaps: &[Snap]) {
        let mut latest = 0_i64;
        for snap in snaps {
            if snap.time_unix > latest {
                latest = snap.time_unix;
            }
        }
        if self.clock <= latest {
            self.clock = latest.saturating_add(1);
        }
    }

    /// Returns whether a missing work tree should be filled from this device's snapshot.
    fn resume_from_repository(
        &mut self,
        repo: &Repository<IndexedFullStatus>,
        snaps: &[Snap],
    ) -> Result<bool, Error> {
        let latest = latest_by_device(snaps);
        let Some(our) = our_snap(&latest, self.device_id.as_str()) else {
            return Ok(false);
        };
        self.cursor = Some(our.id.clone());
        self.last_cursor = Some(cursor_from(our));
        for (path, follow) in &our.state.follows {
            self.follows.entry(path.clone()).or_insert(*follow);
        }
        for (path, snapshot_id) in &our.state.once {
            self.once_done
                .entry(path.clone())
                .or_insert_with(|| snapshot_id.clone());
        }
        for path in &our.state.tombstones {
            self.tombstones.insert(path.clone());
            self.bases.insert(
                path.clone(),
                Base {
                    presence: Presence::Deleted,
                },
            );
        }
        self.conflicts = resolve_conflicts(&our.state.conflicts, &our.id);
        self.published = Some(published_doc(our));
        let empty = work_is_empty(&self.work)?;
        for (path, meta) in &our.files {
            if meta.is_dir || is_marked(path) {
                continue;
            }
            if follow_for(&self.follows, path) != Some(Follow::Always) {
                continue;
            }
            let bytes = read_snapshot_file(repo, our.tree, path)?;
            self.bases.insert(
                path.clone(),
                Base {
                    presence: Presence::Bytes(bytes),
                },
            );
        }
        Ok(empty)
    }

    fn plan_always(
        &self,
        repo: &Repository<IndexedFullStatus>,
        latest: &BTreeMap<String, Snap>,
        found: &BTreeMap<String, Winner>,
        local: &BTreeMap<String, Vec<u8>>,
        materialize: bool,
    ) -> Result<Vec<(String, Plan)>, Error> {
        let mut paths = BTreeSet::new();
        for path in local.keys() {
            paths.insert(path.clone());
        }
        for path in self.bases.keys() {
            if follow_for(&self.follows, path) == Some(Follow::Always) {
                paths.insert(path.clone());
            }
        }
        for path in found.keys() {
            if is_marked(path) {
                continue;
            }
            if follow_for(&self.follows, path) == Some(Follow::Always) {
                paths.insert(path.clone());
            }
        }
        let mut plans = Vec::new();
        for path in paths {
            let local_bytes = local.get(&path).cloned();
            let base = self.bases.get(&path);
            let remote = found.get(&path);
            let local_changed = local_change(base, local_bytes.as_deref(), materialize);
            let remote_bytes = remote_present_bytes(repo, latest, remote, &path)?;
            let remote_changed = remote_content_changed(base, remote, remote_bytes.as_deref());
            if !local_changed && !remote_changed {
                if materialize
                    && local_bytes.is_none()
                    && let Some(Base {
                        presence: Presence::Bytes(bytes),
                    }) = base
                {
                    plans.push((path, Plan::Restore(bytes.clone())));
                }
                continue;
            }
            if remote_changed {
                if remote.is_none() {
                    return Err(Error::repository("remote change without a snapshot"));
                }
                if local_changed
                    && local_bytes.as_deref() != remote_bytes.as_deref()
                    && let Some(bytes) = local_bytes
                {
                    plans.push((
                        path,
                        Plan::Conflict {
                            local: bytes,
                            bytes: remote_bytes,
                        },
                    ));
                    continue;
                }
                plans.push((
                    path,
                    Plan::Adopt {
                        bytes: remote_bytes,
                    },
                ));
                continue;
            }
            plans.push((path, Plan::Upload { bytes: local_bytes }));
        }
        Ok(plans)
    }

    fn apply_remote(&mut self, path: &str, bytes: Option<Vec<u8>>) -> Result<(), Error> {
        if let Some(bytes) = bytes {
            write_work(&self.work, path, &bytes)?;
            self.tombstones.remove(path);
            self.bases.insert(
                path.to_string(),
                Base {
                    presence: Presence::Bytes(bytes),
                },
            );
        } else {
            remove_work(&self.work, path)?;
            self.tombstones.insert(path.to_string());
            self.bases.insert(
                path.to_string(),
                Base {
                    presence: Presence::Deleted,
                },
            );
        }
        Ok(())
    }

    fn apply_once(
        &mut self,
        repo: &Repository<IndexedFullStatus>,
        snaps: &[Snap],
        found: &BTreeMap<String, Winner>,
    ) -> Result<(), Error> {
        let mut downloads = Vec::new();
        let mut paths = BTreeSet::new();
        for (path, follow) in &self.follows {
            if *follow == Follow::Once {
                paths.insert(path.clone());
            }
        }
        for path in found.keys() {
            if follow_for(&self.follows, path) == Some(Follow::Once) && !is_marked(path) {
                paths.insert(path.clone());
            }
        }
        for path in paths {
            if follow_for(&self.follows, &path) != Some(Follow::Once) {
                continue;
            }
            let existing = read_work(&self.work, &path)?;
            if let Some(pinned) = self.once_done.get(&path) {
                if existing.is_none() {
                    downloads.push((path, pinned.clone()));
                }
                continue;
            }
            let Some(Winner::Present { snapshot_id, .. }) = found.get(&path) else {
                continue;
            };
            if existing.is_none() {
                downloads.push((path, snapshot_id.clone()));
            }
        }
        for (path, snapshot_id) in downloads {
            let bytes = read_tree_at(repo, snaps, &snapshot_id, &path)?;
            write_work(&self.work, &path, &bytes)?;
            self.once_done.entry(path).or_insert(snapshot_id);
        }
        Ok(())
    }

    fn publish(
        &mut self,
        repo: &Repository<IndexedFullStatus>,
        latest: &BTreeMap<String, Snap>,
        dirty: bool,
        new_marks: Vec<(StoredConflict, Vec<u8>)>,
    ) -> Result<SyncOutcome, Error> {
        let extras: Vec<StoredConflict> = new_marks
            .iter()
            .map(|(conflict, _)| conflict.clone())
            .collect();
        let doc = self.state_doc(&extras);
        let state_changed = self.published.as_ref() != Some(&doc);
        if self.cursor.is_some() && !state_changed && !dirty {
            return Ok(SyncOutcome {
                cursor: self.last_cursor.clone(),
                conflicts: Vec::new(),
            });
        }

        let stage_parent = self
            .work
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| self.work.clone());
        let stage = stage_parent.join(format!(".satchel-stage-{}", self.device_id.as_str()));
        let _cleanup = DeleteOnDrop(stage.clone());
        fill_stage(&stage, &self.work, &self.follows, &new_marks)?;

        let mut parents = Vec::new();
        if let Some(cursor) = &self.cursor {
            parents.push(cursor.clone());
        }
        for snap in latest.values() {
            if snap.device_id != self.device_id.as_str() && !parents.contains(&snap.id) {
                parents.push(snap.id.clone());
            }
        }
        let parent_opts = if parents.is_empty() {
            ParentOptions::default()
                .force(true)
                .group_by(SnapshotGroupCriterion::new())
        } else {
            ParentOptions::default()
                .group_by(SnapshotGroupCriterion::new())
                .parents(parents.clone())
        };
        let zoned = Timestamp::from_second(self.clock)
            .map_err(Error::repository)?
            .to_zoned(TimeZone::UTC);
        let description = serde_json::to_string(&doc).map_err(Error::repository)?;
        let snapshot = SnapshotOptions::default()
            .host(Some(self.device_id.as_str().to_string()))
            .command(Some("satchel".to_string()))
            .time(Some(zoned))
            .description(Some(description))
            .to_snapshot()
            .map_err(Error::repository)?;
        let stage_text = canonical_text(&stage)?;
        let paths = PathList::from_string(&stage_text)
            .map_err(Error::repository)?
            .sanitize()
            .map_err(Error::repository)?;
        let saved = repo
            .backup(
                &BackupOptions::default()
                    .parent_opts(parent_opts)
                    .as_path(PathBuf::from(TREE_ROOT)),
                &paths,
                snapshot,
            )
            .map_err(Error::repository)?;
        let snapshot_id = saved.id.to_hex().as_str().to_string();
        if saved.hostname != self.device_id.as_str() {
            return Err(Error::repository(
                "snapshot host was not the opaque device id",
            ));
        }
        let parent_id = saved
            .parent
            .map(|parent| parent.to_hex().as_str().to_string());
        if parents.is_empty() {
            if parent_id.is_some() {
                return Err(Error::repository("snapshot recorded an unexpected parent"));
            }
        } else if parent_id.as_deref() != Some(parents[0].as_str()) {
            return Err(Error::repository("snapshot parent was not the sync cursor"));
        }

        let mut stored = extras;
        for conflict in &mut stored {
            if conflict.snapshot_id.is_none() {
                conflict.snapshot_id = Some(snapshot_id.clone());
            }
        }
        self.conflicts.append(&mut stored.clone());
        let mut published = doc;
        published.conflicts = self.conflicts.clone();
        self.published = Some(published);
        self.cursor = Some(snapshot_id.clone());
        let cursor = Cursor {
            snapshot_id: snapshot_id.clone(),
            parent_id,
            device_id: self.device_id.as_str().to_string(),
            time_unix: self.clock,
        };
        self.last_cursor = Some(cursor.clone());
        self.remember_published_tree()?;
        let mut conflicts = Vec::new();
        for (conflict, _) in new_marks {
            conflicts.push(MarkedConflict {
                path: conflict.path,
                marked_path: conflict.marked_path,
                device_id: conflict.device_id,
                snapshot_id: snapshot_id.clone(),
            });
        }
        Ok(SyncOutcome {
            cursor: Some(cursor),
            conflicts,
        })
    }

    fn state_doc(&self, extras: &[StoredConflict]) -> StateDoc {
        let mut conflicts = self.conflicts.clone();
        for extra in extras {
            conflicts.push(extra.clone());
        }
        StateDoc {
            satchel: STATE_VERSION,
            follows: self.follows.clone(),
            tombstones: self.tombstones.clone(),
            once: self.once_done.clone(),
            conflicts,
        }
    }

    fn remember_published_tree(&mut self) -> Result<(), Error> {
        let files = scan_followed(&self.work, &self.follows, Follow::Always)?;
        for (path, bytes) in files {
            self.bases.insert(
                path,
                Base {
                    presence: Presence::Bytes(bytes),
                },
            );
        }
        let tombstones = self.tombstones.clone();
        for path in tombstones {
            self.bases.insert(
                path,
                Base {
                    presence: Presence::Deleted,
                },
            );
        }
        Ok(())
    }
}

struct DeleteOnDrop(PathBuf);

impl Drop for DeleteOnDrop {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

impl Winner {
    fn rank(&self) -> (i64, &str) {
        match self {
            Self::Present {
                time_unix,
                snapshot_id,
                ..
            }
            | Self::Deleted {
                time_unix,
                snapshot_id,
                ..
            } => (*time_unix, snapshot_id.as_str()),
        }
    }
}

fn remote_content_changed(
    base: Option<&Base>,
    remote: Option<&Winner>,
    remote_bytes: Option<&[u8]>,
) -> bool {
    match (base, remote) {
        (None, None) => false,
        (None, Some(_)) => true,
        (Some(_), None) => false,
        (Some(base), Some(Winner::Deleted { .. })) => !matches!(base.presence, Presence::Deleted),
        (Some(base), Some(Winner::Present { .. })) => match &base.presence {
            Presence::Deleted => true,
            Presence::Bytes(old) => remote_bytes != Some(old.as_slice()),
        },
    }
}

fn local_change(base: Option<&Base>, local: Option<&[u8]>, materialize: bool) -> bool {
    match (base, local) {
        (None, None) => false,
        (None, Some(_)) => true,
        (Some(base), None) => {
            if materialize && matches!(base.presence, Presence::Bytes(_)) {
                false
            } else {
                matches!(base.presence, Presence::Bytes(_))
            }
        }
        (Some(base), Some(bytes)) => match &base.presence {
            Presence::Deleted => true,
            Presence::Bytes(old) => old != bytes,
        },
    }
}

fn remote_present_bytes(
    repo: &Repository<IndexedFullStatus>,
    latest: &BTreeMap<String, Snap>,
    winner: Option<&Winner>,
    path: &str,
) -> Result<Option<Vec<u8>>, Error> {
    let Some(winner) = winner else {
        return Ok(None);
    };
    match winner {
        Winner::Deleted { .. } => Ok(None),
        Winner::Present { snapshot_id, .. } => {
            let snap = latest
                .values()
                .find(|snap| snap.id == *snapshot_id)
                .ok_or_else(|| Error::repository(format!("missing snapshot {snapshot_id}")))?;
            let bytes = read_snapshot_file(repo, snap.tree, path)?;
            Ok(Some(bytes))
        }
    }
}

/// Read cursors from every satchel snapshot.
/// Publish every regular file in `folder` as Sincronizar sempre.
///
/// The repository is created when its config object is absent. The password is
/// any key that already opens an existing repository, or the recovery password
/// used to create it.
pub fn publish_folder(
    location: &Location,
    password: &str,
    device_id: &DeviceId,
    folder: &Path,
) -> Result<SyncOutcome, Error> {
    if !folder.is_dir() {
        return Err(Error::SourceDirectory {
            path: folder.to_path_buf(),
        });
    }
    if !location.config_present()? {
        init(location, password, device_id)?;
    }
    let mut client = Client::open(device_id.clone(), folder, password)?;
    let files = regular_files(folder)?;
    for path in files {
        client.follow(&path, Follow::Always)?;
    }
    client.sync(location)
}

pub fn cursors(location: &Location, password: &str) -> Result<Vec<Cursor>, Error> {
    let repo = open_indexed(location, password)?;
    let snaps = load_snaps(&repo)?;
    let mut listed = Vec::new();
    for snap in snaps {
        listed.push(cursor_from(&snap));
    }
    Ok(listed)
}

/// Write the newest snapshot into an empty directory.
///
/// Only the recovery password and the repository location are required.
/// This follows the same snapshot `restic restore latest` would choose.
pub fn restore_latest(
    location: &Location,
    password: &str,
    destination: &Path,
) -> Result<(), Error> {
    let repo = open_indexed(location, password)?;
    let snaps = load_snaps(&repo)?;
    let Some(snap) = snaps.iter().max_by(|left, right| {
        (left.time_unix, left.id.as_str()).cmp(&(right.time_unix, right.id.as_str()))
    }) else {
        return Err(Error::repository("repository has no satchel snapshot"));
    };
    fs::create_dir_all(destination).map_err(Error::repository)?;
    for (path, meta) in &snap.files {
        if meta.is_dir {
            continue;
        }
        let bytes = read_snapshot_file(&repo, snap.tree, path)?;
        write_under(destination, path, &bytes)?;
    }
    Ok(())
}

/// Read the bytes this device keeps for `path`.
///
/// A Baixar agora pin names the snapshot that was downloaded. Without a pin,
/// the snapshot that currently holds the file is used. A later mark does not
/// have to copy those bytes into its own tree.
pub fn read_followed(
    location: &Location,
    password: &str,
    device_id: &DeviceId,
    path: &str,
) -> Result<Vec<u8>, Error> {
    let path = normalize_path(path)?;
    let repo = open_indexed(location, password)?;
    let snaps = load_snaps(&repo)?;
    let latest = latest_by_device(&snaps);
    if let Some(our) = our_snap(&latest, device_id.as_str())
        && let Some(pinned) = our.state.once.get(&path)
    {
        return read_tree_at(&repo, &snaps, pinned, &path);
    }
    let found = winners(&latest);
    let Some(Winner::Present { snapshot_id, .. }) = found.get(&path) else {
        return Err(Error::MissingFile {
            snapshot: String::new(),
            path,
        });
    };
    read_tree_at(&repo, &snaps, snapshot_id, &path)
}

/// Read one file from a snapshot. This is the history path, not the sync queue.
pub fn read_version(
    location: &Location,
    password: &str,
    snapshot_id: &str,
    path: &str,
) -> Result<Vec<u8>, Error> {
    let path = normalize_path(path)?;
    let repo = open_indexed(location, password)?;
    let snaps = load_snaps(&repo)?;
    read_tree_at(&repo, &snaps, snapshot_id, &path)
}

fn read_tree_at(
    repo: &Repository<IndexedFullStatus>,
    snaps: &[Snap],
    snapshot_id: &str,
    path: &str,
) -> Result<Vec<u8>, Error> {
    let snap = snaps
        .iter()
        .find(|snap| snap.id == snapshot_id)
        .ok_or_else(|| Error::MissingFile {
            snapshot: snapshot_id.to_string(),
            path: path.to_string(),
        })?;
    read_snapshot_file(repo, snap.tree, path).map_err(|_| Error::MissingFile {
        snapshot: snapshot_id.to_string(),
        path: path.to_string(),
    })
}

fn cursor_from(snap: &Snap) -> Cursor {
    Cursor {
        snapshot_id: snap.id.clone(),
        parent_id: snap.parent_id.clone(),
        device_id: snap.device_id.clone(),
        time_unix: snap.time_unix,
    }
}

fn load_snaps(repo: &Repository<IndexedFullStatus>) -> Result<Vec<Snap>, Error> {
    let snapshots = repo.get_all_snapshots().map_err(Error::repository)?;
    let mut snaps = Vec::new();
    for snapshot in snapshots {
        let Some(state) = parse_state(&snapshot)? else {
            continue;
        };
        let id = snapshot.id.to_hex().as_str().to_string();
        let files = list_tree(repo, snapshot.tree)?;
        snaps.push(Snap {
            id,
            parent_id: snapshot
                .parent
                .map(|parent| parent.to_hex().as_str().to_string()),
            device_id: snapshot.hostname,
            time_unix: snapshot.time.timestamp().as_second(),
            tree: snapshot.tree,
            state,
            files,
        });
    }
    Ok(snaps)
}

fn parse_state(snapshot: &SnapshotFile) -> Result<Option<StateDoc>, Error> {
    let Some(text) = snapshot.description.as_deref() else {
        return Ok(None);
    };
    if !text.contains("\"satchel\"") {
        return Ok(None);
    }
    let doc: StateDoc = serde_json::from_str(text).map_err(Error::repository)?;
    if doc.satchel != STATE_VERSION {
        return Err(Error::repository(format!(
            "unsupported snapshot state version {}",
            doc.satchel
        )));
    }
    Ok(Some(doc))
}

fn list_tree(
    repo: &Repository<IndexedFullStatus>,
    tree: TreeId,
) -> Result<BTreeMap<String, FileMeta>, Error> {
    let root = repo
        .node_from_path(tree, Path::new(""))
        .map_err(Error::repository)?;
    let listing = repo
        .ls(&root, &LsOptions::default())
        .map_err(Error::repository)?;
    let mut files = BTreeMap::new();
    for item in listing {
        let (path, node) = item.map_err(Error::repository)?;
        if !node.is_file() && !node.is_dir() {
            continue;
        }
        let text = path_to_rel(&path)?;
        let Some(text) = logical_path(&text) else {
            continue;
        };
        if text.is_empty() || is_marked(&text) {
            continue;
        }
        files.insert(
            text,
            FileMeta {
                size: node.meta.size,
                is_dir: node.is_dir(),
            },
        );
    }
    Ok(files)
}

fn latest_by_device(snaps: &[Snap]) -> BTreeMap<String, Snap> {
    let mut latest: BTreeMap<String, Snap> = BTreeMap::new();
    for snap in snaps {
        let replace = match latest.get(&snap.device_id) {
            None => true,
            Some(current) => {
                (snap.time_unix, snap.id.as_str()) > (current.time_unix, current.id.as_str())
            }
        };
        if replace {
            latest.insert(snap.device_id.clone(), snap.clone());
        }
    }
    latest
}

fn our_snap<'a>(latest: &'a BTreeMap<String, Snap>, device_id: &str) -> Option<&'a Snap> {
    latest.get(device_id)
}

fn winners(latest: &BTreeMap<String, Snap>) -> BTreeMap<String, Winner> {
    let mut found = BTreeMap::new();
    for snap in latest.values() {
        for (path, meta) in &snap.files {
            if meta.is_dir || is_marked(path) || snap.state.tombstones.contains(path) {
                continue;
            }
            consider(
                &mut found,
                path,
                Winner::Present {
                    snapshot_id: snap.id.clone(),
                    time_unix: snap.time_unix,
                    size: meta.size,
                },
            );
        }
        for path in &snap.state.tombstones {
            consider(
                &mut found,
                path,
                Winner::Deleted {
                    snapshot_id: snap.id.clone(),
                    time_unix: snap.time_unix,
                },
            );
        }
    }
    found
}

fn consider(found: &mut BTreeMap<String, Winner>, path: &str, next: Winner) {
    match found.get(path) {
        Some(current) if current.rank() >= next.rank() => {}
        _ => {
            found.insert(path.to_string(), next);
        }
    }
}

fn merged_follows(
    session: &BTreeMap<String, Follow>,
    our: Option<&Snap>,
) -> BTreeMap<String, Follow> {
    let mut follows = our
        .map(|snap| snap.state.follows.clone())
        .unwrap_or_default();
    for (path, follow) in session {
        follows.insert(path.clone(), *follow);
    }
    follows
}

fn devices_keeping(latest: &BTreeMap<String, Snap>, path: &str) -> Vec<String> {
    let mut devices = Vec::new();
    for snap in latest.values() {
        let Some(follow) = follow_for(&snap.state.follows, path) else {
            continue;
        };
        if matches!(follow, Follow::Once | Follow::Always) {
            devices.push(snap.device_id.clone());
        }
    }
    devices.sort();
    devices
}

fn collect_conflicts(latest: &BTreeMap<String, Snap>) -> Vec<MarkedConflict> {
    let mut conflicts = Vec::new();
    let mut seen = BTreeSet::new();
    for snap in latest.values() {
        for conflict in resolve_conflicts(&snap.state.conflicts, &snap.id) {
            let Some(snapshot_id) = conflict.snapshot_id.clone() else {
                continue;
            };
            if !seen.insert(conflict.marked_path.clone()) {
                continue;
            }
            conflicts.push(MarkedConflict {
                path: conflict.path,
                marked_path: conflict.marked_path,
                device_id: conflict.device_id,
                snapshot_id,
            });
        }
    }
    conflicts
}

fn resolve_conflicts(conflicts: &[StoredConflict], snapshot_id: &str) -> Vec<StoredConflict> {
    let mut resolved = Vec::new();
    for conflict in conflicts {
        let mut conflict = conflict.clone();
        if conflict.snapshot_id.is_none() {
            conflict.snapshot_id = Some(snapshot_id.to_string());
        }
        resolved.push(conflict);
    }
    resolved
}

fn published_doc(snap: &Snap) -> StateDoc {
    StateDoc {
        satchel: STATE_VERSION,
        follows: snap.state.follows.clone(),
        tombstones: snap.state.tombstones.clone(),
        once: snap.state.once.clone(),
        conflicts: resolve_conflicts(&snap.state.conflicts, &snap.id),
    }
}

fn follow_for(follows: &BTreeMap<String, Follow>, path: &str) -> Option<Follow> {
    if let Some(follow) = follows.get(path) {
        return Some(*follow);
    }
    let mut best: Option<(&str, Follow)> = None;
    for (mark, follow) in follows {
        if path_is_inside(path, mark) {
            let replace = match best {
                None => true,
                Some((current, _)) => mark.len() > current.len(),
            };
            if replace {
                best = Some((mark.as_str(), *follow));
            }
        }
    }
    best.map(|(_, follow)| follow)
}

fn path_is_inside(path: &str, dir: &str) -> bool {
    let Some(rest) = path.strip_prefix(dir) else {
        return false;
    };
    rest.starts_with('/')
}

fn repo_path(logical: &str) -> String {
    format!("{TREE_ROOT}/{logical}")
}

fn logical_path(repo_path: &str) -> Option<String> {
    if repo_path == TREE_ROOT {
        return None;
    }
    repo_path
        .strip_prefix(&format!("{TREE_ROOT}/"))
        .map(ToString::to_string)
}

fn is_marked(path: &str) -> bool {
    let name = path.rsplit_once('/').map(|(_, name)| name).unwrap_or(path);
    name.contains(CONFLICT_MARK)
}

fn normalize_path(path: &str) -> Result<String, Error> {
    let path = path.trim_end_matches('/');
    if path.is_empty() || Path::new(path).is_absolute() {
        return Err(Error::Path {
            path: path.to_string(),
        });
    }
    let mut parts = Vec::new();
    for part in path.split('/') {
        if part.is_empty() || part == "." || part == ".." || part.contains('\\') {
            return Err(Error::Path {
                path: path.to_string(),
            });
        }
        parts.push(part);
    }
    Ok(parts.join("/"))
}

fn path_to_rel(path: &Path) -> Result<String, Error> {
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => {
                let text = part.to_str().ok_or_else(|| {
                    Error::repository(format!("path `{}` is not valid unicode", path.display()))
                })?;
                parts.push(text);
            }
            Component::CurDir => {}
            Component::RootDir | Component::ParentDir | Component::Prefix(_) => {
                return Err(Error::Path {
                    path: path.display().to_string(),
                });
            }
        }
    }
    Ok(parts.join("/"))
}

fn work_path(work: &Path, rel: &str) -> Result<PathBuf, Error> {
    let rel = normalize_path(rel)?;
    let mut path = work.to_path_buf();
    for part in rel.split('/') {
        path.push(part);
    }
    Ok(path)
}

fn read_work(work: &Path, rel: &str) -> Result<Option<Vec<u8>>, Error> {
    let path = work_path(work, rel)?;
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(Error::repository(err)),
    };
    if !metadata.file_type().is_file() {
        return Ok(None);
    }
    let bytes = stable_read(&path)?;
    Ok(Some(bytes))
}

fn stable_read(path: &Path) -> Result<Vec<u8>, Error> {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| Error::repository("file name is not valid unicode"))?;
    let temp = path.with_file_name(format!(".{name}.satchel-read"));
    fs::copy(path, &temp).map_err(Error::repository)?;
    let bytes = fs::read(&temp).map_err(Error::repository);
    fs::remove_file(&temp).map_err(Error::repository)?;
    bytes
}

fn write_work(work: &Path, rel: &str, bytes: &[u8]) -> Result<(), Error> {
    write_under(work, rel, bytes)
}

fn write_under(root: &Path, rel: &str, bytes: &[u8]) -> Result<(), Error> {
    let path = work_path(root, rel)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(Error::repository)?;
    }
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| Error::repository("file name is not valid unicode"))?;
    let temp = path.with_file_name(format!(".{name}.satchel-write"));
    fs::write(&temp, bytes).map_err(Error::repository)?;
    fs::rename(&temp, &path).map_err(Error::repository)?;
    Ok(())
}

fn remove_work(work: &Path, rel: &str) -> Result<(), Error> {
    let path = work_path(work, rel)?;
    match fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.file_type().is_file() => {
            fs::remove_file(&path).map_err(Error::repository)?;
        }
        Ok(_) => {}
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => return Err(Error::repository(err)),
    }
    Ok(())
}

fn work_is_empty(work: &Path) -> Result<bool, Error> {
    let mut pending = vec![work.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(&dir).map_err(Error::repository)? {
            let entry = entry.map_err(Error::repository)?;
            let metadata = fs::symlink_metadata(entry.path()).map_err(Error::repository)?;
            if metadata.file_type().is_symlink() {
                continue;
            }
            if metadata.is_dir() {
                pending.push(entry.path());
                continue;
            }
            if metadata.is_file() {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

fn regular_files(folder: &Path) -> Result<Vec<String>, Error> {
    let mut files = Vec::new();
    let mut pending = vec![(folder.to_path_buf(), String::new())];
    while let Some((dir, prefix)) = pending.pop() {
        for entry in fs::read_dir(&dir).map_err(Error::repository)? {
            let entry = entry.map_err(Error::repository)?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            if name.ends_with(".satchel-read") || name.ends_with(".satchel-write") {
                continue;
            }
            let rel = if prefix.is_empty() {
                name.to_string()
            } else {
                format!("{prefix}/{name}")
            };
            let metadata = fs::symlink_metadata(entry.path()).map_err(Error::repository)?;
            if metadata.file_type().is_symlink() {
                continue;
            }
            if metadata.is_dir() {
                pending.push((entry.path(), rel));
                continue;
            }
            if !metadata.is_file() || is_marked(&rel) {
                continue;
            }
            files.push(rel);
        }
    }
    Ok(files)
}

fn scan_followed(
    work: &Path,
    follows: &BTreeMap<String, Follow>,
    want: Follow,
) -> Result<BTreeMap<String, Vec<u8>>, Error> {
    let mut found = BTreeMap::new();
    let mut pending = vec![(work.to_path_buf(), String::new())];
    while let Some((dir, prefix)) = pending.pop() {
        for entry in fs::read_dir(&dir).map_err(Error::repository)? {
            let entry = entry.map_err(Error::repository)?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            if name.ends_with(".satchel-read") || name.ends_with(".satchel-write") {
                continue;
            }
            let rel = if prefix.is_empty() {
                name.to_string()
            } else {
                format!("{prefix}/{name}")
            };
            let metadata = fs::symlink_metadata(entry.path()).map_err(Error::repository)?;
            if metadata.file_type().is_symlink() {
                continue;
            }
            if metadata.is_dir() {
                pending.push((entry.path(), rel));
                continue;
            }
            if !metadata.is_file() || is_marked(&rel) {
                continue;
            }
            if follow_for(follows, &rel) != Some(want) {
                continue;
            }
            let bytes = stable_read(&entry.path())?;
            found.insert(rel, bytes);
        }
    }
    Ok(found)
}

fn fill_stage(
    stage: &Path,
    work: &Path,
    follows: &BTreeMap<String, Follow>,
    marks: &[(StoredConflict, Vec<u8>)],
) -> Result<(), Error> {
    fs::create_dir_all(stage).map_err(Error::repository)?;
    let files = scan_followed(work, follows, Follow::Always)?;
    for (rel, bytes) in files {
        write_under(stage, &rel, &bytes)?;
    }
    for (conflict, bytes) in marks {
        write_under(stage, &conflict.marked_path, bytes)?;
    }
    Ok(())
}

fn canonical_text(path: &Path) -> Result<String, Error> {
    let canonical = fs::canonicalize(path).map_err(Error::repository)?;
    canonical
        .into_os_string()
        .into_string()
        .map_err(|_| Error::repository(format!("path `{}` is not valid unicode", path.display())))
}

fn read_snapshot_file(
    repo: &Repository<IndexedFullStatus>,
    tree: TreeId,
    path: &str,
) -> Result<Vec<u8>, Error> {
    let node = repo
        .node_from_path(tree, Path::new(&repo_path(path)))
        .map_err(Error::repository)?;
    if !node.is_file() {
        return Err(Error::repository(format!("`{path}` is not a file")));
    }
    let open = repo.open_file(&node).map_err(Error::repository)?;
    let size = usize::try_from(node.meta.size)
        .map_err(|_| Error::repository(format!("`{path}` is too large")))?;
    let bytes = repo
        .read_file_at(&open, 0, size)
        .map_err(Error::repository)?;
    Ok(bytes.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folder_follow_covers_a_new_child_and_an_explicit_child_wins() {
        let mut follows = BTreeMap::new();
        follows.insert("docs".to_string(), Follow::Always);
        follows.insert("docs/secret.txt".to_string(), Follow::See);
        assert_eq!(follow_for(&follows, "docs/a.txt"), Some(Follow::Always));
        assert_eq!(follow_for(&follows, "docs/secret.txt"), Some(Follow::See));
        assert_eq!(follow_for(&follows, "other.txt"), None);
    }

    #[test]
    fn marked_names_and_parent_segments_are_rejected() {
        assert!(is_marked(
            "docs/a.txt.satchel-conflict-0123456789abcdef0123456789abcdef-10"
        ));
        assert!(normalize_path("../a.txt").is_err());
        assert!(normalize_path("/etc/passwd").is_err());
        assert_eq!(normalize_path("docs/a.txt").expect("path"), "docs/a.txt");
    }
}
