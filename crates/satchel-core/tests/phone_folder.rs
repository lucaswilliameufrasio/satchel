//! A phone folder synced through the copy round, against a desktop folder.

use std::fs;
use std::path::Path;

use satchel_core::{
    Client, DeviceId, DirectoryTree, Error, FolderTree, Follow, Location, MirrorReport, TreeFile,
    add_device_key, init, read_version, sync_folder, sync_mirrored,
};
use tempfile::tempdir;

const DESKTOP: &str = "0123456789abcdef0123456789abcdef";
const PHONE: &str = "fedcba9876543210fedcba9876543210";
const RECOVERY: &str = "recovery-password";
const PHONE_KEY: &str = "phone-key-password";

struct Rig {
    location: Location,
    desktop: DeviceId,
    phone: DeviceId,
    desktop_folder: std::path::PathBuf,
    phone_work: std::path::PathBuf,
    phone_folder: std::path::PathBuf,
    manifest: std::path::PathBuf,
    _dir: tempfile::TempDir,
}

impl Rig {
    fn new() -> Self {
        let dir = tempdir().expect("temp dir");
        let repository = dir.path().join("repository");
        fs::create_dir_all(&repository).expect("repository");
        let location = Location::Directory(repository);
        let desktop = DeviceId::parse(DESKTOP).expect("desktop");
        let phone = DeviceId::parse(PHONE).expect("phone");
        init(&location, RECOVERY, &desktop).expect("init");
        add_device_key(&location, RECOVERY, &phone, PHONE_KEY).expect("phone key");
        let rig = Self {
            location,
            desktop,
            phone,
            desktop_folder: dir.path().join("desktop"),
            phone_work: dir.path().join("phone-work"),
            phone_folder: dir.path().join("phone-folder"),
            manifest: dir.path().join("phone-folder.manifest"),
            _dir: dir,
        };
        fs::create_dir_all(&rig.desktop_folder).expect("desktop folder");
        fs::create_dir_all(&rig.phone_folder).expect("phone folder");
        rig
    }

    fn desktop_sync(&self) {
        sync_folder(
            &self.location,
            RECOVERY,
            &self.desktop,
            &self.desktop_folder,
        )
        .expect("desktop sync");
    }

    fn phone_client(&self) -> Client {
        Client::open(self.phone.clone(), &self.phone_work, PHONE_KEY).expect("phone client")
    }

    fn phone_round(&self) -> MirrorReport {
        self.phone_round_with(&mut DirectoryTree::new(&self.phone_folder))
            .expect("phone round")
    }

    fn phone_round_with(&self, tree: &mut dyn FolderTree) -> Result<MirrorReport, Error> {
        let mut client = self.phone_client();
        sync_mirrored(&mut client, &self.location, tree, &self.manifest)
    }
}

fn put(root: &Path, rel: &str, bytes: &[u8]) {
    let path = root.join(rel);
    fs::create_dir_all(path.parent().expect("parent")).expect("parent dir");
    fs::write(path, bytes).expect("write");
}

fn read(root: &Path, rel: &str) -> Option<Vec<u8>> {
    fs::read(root.join(rel)).ok()
}

#[test]
fn the_phone_folder_and_a_desktop_folder_follow_each_other() {
    let rig = Rig::new();

    // The person drops two files in the phone folder.
    put(&rig.phone_folder, "photo.jpg", b"photo-bytes");
    put(&rig.phone_folder, "notes/a.txt", b"first");
    let first = rig.phone_round();
    assert_eq!(first.imported, vec!["notes/a.txt", "photo.jpg"]);
    assert!(first.exported.is_empty());
    // A real round carries a real date, not the Unix epoch.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("time")
        .as_secs() as i64;
    let stamped = first
        .outcome
        .as_ref()
        .expect("synced")
        .cursor
        .as_ref()
        .expect("cursor")
        .time_unix;
    assert!(
        (now - 60..=now + 60).contains(&stamped),
        "snapshot time {stamped}"
    );
    rig.desktop_sync();
    assert_eq!(
        read(&rig.desktop_folder, "photo.jpg").as_deref(),
        Some(&b"photo-bytes"[..])
    );
    assert_eq!(
        read(&rig.desktop_folder, "notes/a.txt").as_deref(),
        Some(&b"first"[..])
    );

    // A round with nothing new moves nothing and does not open the repository:
    // a client with a wrong password still gets through it.
    let woken = rig.phone_round();
    assert!(!woken.quiet, "the desktop's snapshot wakes one round");
    assert!(woken.imported.is_empty() && woken.exported.is_empty());
    let idle = rig.phone_round();
    assert!(idle.quiet, "nothing changed since the last round");
    assert!(idle.imported.is_empty() && idle.exported.is_empty());
    assert!(idle.deleted_here.is_empty() && idle.deleted_there.is_empty());
    let mut stranger =
        Client::open(rig.phone.clone(), &rig.phone_work, "not-the-password").expect("client");
    let quiet = sync_mirrored(
        &mut stranger,
        &rig.location,
        &mut DirectoryTree::new(&rig.phone_folder),
        &rig.manifest,
    )
    .expect("quiet round needs no key");
    assert!(quiet.quiet);

    // The desktop edits a file the phone keeps and adds one the phone never saw.
    put(
        &rig.desktop_folder,
        "notes/a.txt",
        b"second, from the desktop",
    );
    put(&rig.desktop_folder, "big-video.mov", b"video-bytes");
    rig.desktop_sync();
    let pulled = rig.phone_round();
    assert!(!pulled.quiet, "a new snapshot wakes the round");
    assert_eq!(pulled.exported, vec!["notes/a.txt"]);
    assert_eq!(
        read(&rig.phone_folder, "notes/a.txt").as_deref(),
        Some(&b"second, from the desktop"[..])
    );
    assert!(
        read(&rig.phone_folder, "big-video.mov").is_none(),
        "a file the phone never marked stays in the catalog only"
    );

    // Marking it to download once lands it in the folder.
    let mut client = rig.phone_client();
    client.follow("big-video.mov", Follow::Once).expect("mark");
    let marked = sync_mirrored(
        &mut client,
        &rig.location,
        &mut DirectoryTree::new(&rig.phone_folder),
        &rig.manifest,
    )
    .expect("marked round");
    assert_eq!(marked.exported, vec!["big-video.mov"]);
    assert_eq!(
        read(&rig.phone_folder, "big-video.mov").as_deref(),
        Some(&b"video-bytes"[..])
    );

    // The person deletes a photo on the phone: the desktop loses it too.
    fs::remove_file(rig.phone_folder.join("photo.jpg")).expect("delete");
    let deleted = rig.phone_round();
    assert!(!deleted.quiet, "a change in the folder wakes the round");
    assert_eq!(deleted.deleted_here, vec!["photo.jpg"]);
    rig.desktop_sync();
    assert!(read(&rig.desktop_folder, "photo.jpg").is_none());

    // The desktop deletes a note: the phone folder loses it.
    fs::remove_file(rig.desktop_folder.join("notes/a.txt")).expect("delete");
    rig.desktop_sync();
    let removed = rig.phone_round();
    assert_eq!(removed.deleted_there, vec!["notes/a.txt"]);
    assert!(read(&rig.phone_folder, "notes/a.txt").is_none());
    assert!(
        read(&rig.phone_folder, "big-video.mov").is_some(),
        "the other files stay"
    );
}

#[test]
fn an_unreachable_folder_deletes_nothing() {
    let rig = Rig::new();
    put(&rig.phone_folder, "keep.txt", b"keep");
    rig.phone_round();
    rig.desktop_sync();

    // A revoked permission or an unmounted card: the folder lists nothing.
    fs::remove_file(rig.phone_folder.join("keep.txt")).expect("empty");
    let refused = rig.phone_round_with(&mut DirectoryTree::new(&rig.phone_folder));
    assert!(matches!(refused, Err(Error::FolderUnavailable)));
    fs::remove_dir(&rig.phone_folder).expect("gone");
    let gone = rig.phone_round_with(&mut DirectoryTree::new(&rig.phone_folder));
    assert!(matches!(gone, Err(Error::FolderUnavailable)));

    rig.desktop_sync();
    assert_eq!(
        read(&rig.desktop_folder, "keep.txt").as_deref(),
        Some(&b"keep"[..]),
        "the desktop still has the file"
    );
}

/// A folder where the person saves a file while the round is running.
struct EditsDuringTheRound {
    inner: DirectoryTree,
    root: std::path::PathBuf,
    lists: usize,
}

impl FolderTree for EditsDuringTheRound {
    fn list(&mut self) -> Result<Vec<TreeFile>, Error> {
        self.lists += 1;
        if self.lists == 2 {
            put(
                &self.root,
                "shared.txt",
                b"typed on the phone during the round",
            );
        }
        self.inner.list()
    }
    fn copy_in(&mut self, path: &str, dest: &std::path::Path) -> Result<(), Error> {
        self.inner.copy_in(path, dest)
    }
    fn copy_out(&mut self, source: &std::path::Path, path: &str) -> Result<(), Error> {
        self.inner.copy_out(source, path)
    }
    fn remove(&mut self, path: &str) -> Result<(), Error> {
        self.inner.remove(path)
    }
}

#[test]
fn an_edit_during_the_round_is_not_overwritten_and_is_kept_as_a_conflict() {
    let rig = Rig::new();
    put(&rig.phone_folder, "shared.txt", b"v1");
    rig.phone_round();
    rig.desktop_sync();
    put(&rig.desktop_folder, "shared.txt", b"v2 from the desktop");
    rig.desktop_sync();

    let mut tree = EditsDuringTheRound {
        inner: DirectoryTree::new(&rig.phone_folder),
        root: rig.phone_folder.clone(),
        lists: 0,
    };
    let round = rig.phone_round_with(&mut tree).expect("round");
    assert_eq!(round.deferred, vec!["shared.txt"]);
    assert!(round.exported.is_empty());
    assert_eq!(
        read(&rig.phone_folder, "shared.txt").as_deref(),
        Some(&b"typed on the phone during the round"[..]),
        "the person's edit is kept"
    );

    // The person typed over v1 without seeing v2, so the next round is a real
    // conflict: the folder gets v2 and the typed text is kept as a marked copy.
    let next = rig.phone_round();
    assert_eq!(next.imported, vec!["shared.txt"]);
    assert_eq!(next.outcome.as_ref().expect("synced").conflicts.len(), 1);
    assert_eq!(
        next.outcome.as_ref().expect("synced").conflicts[0].path,
        "shared.txt"
    );
    assert_eq!(
        read(&rig.phone_folder, "shared.txt").as_deref(),
        Some(&b"v2 from the desktop"[..])
    );
    let marked = rig
        .phone_client()
        .conflicts(&rig.location)
        .expect("conflicts")
        .into_iter()
        .find(|conflict| conflict.path == "shared.txt")
        .expect("marked copy");
    let snapshot = marked.snapshot_id;
    assert_eq!(
        read_version(&rig.location, PHONE_KEY, &snapshot, &marked.marked_path).expect("bytes"),
        b"typed on the phone during the round"
    );
    rig.desktop_sync();
    assert_eq!(
        read(&rig.desktop_folder, "shared.txt").as_deref(),
        Some(&b"v2 from the desktop"[..])
    );
}
