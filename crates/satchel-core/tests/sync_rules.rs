use std::fs;
use std::os::unix::fs::symlink;
use std::path::Path;

use satchel_core::{Client, DeviceId, Follow, Location, cursors, init, read_version};
use tempfile::tempdir;

const DEVICE_A: &str = "0123456789abcdef0123456789abcdef";
const DEVICE_B: &str = "fedcba9876543210fedcba9876543210";
const PASSWORD: &str = "recovery-password";

#[test]
fn two_clients_follow_see_once_always_delete_and_conflict() {
    let workspace = tempdir().expect("temp dir");
    let repository = workspace.path().join("repository");
    fs::create_dir_all(&repository).expect("repository");
    let location = Location::Directory(repository);
    let device_a = DeviceId::parse(DEVICE_A).expect("device a");
    let device_b = DeviceId::parse(DEVICE_B).expect("device b");
    init(&location, PASSWORD, &device_a).expect("init");

    let mut clock = 1_700_000_000_i64;
    let work_a = workspace.path().join("a");
    let work_b = workspace.path().join("b");
    let mut client_a = Client::open(device_a.clone(), &work_a, PASSWORD).expect("client a");
    let mut client_b = Client::open(device_b, &work_b, PASSWORD).expect("client b");

    fs::create_dir_all(work_a.join("notes")).expect("notes");
    fs::write(work_a.join("notes/a.txt"), b"one").expect("note");
    client_a
        .follow("notes/a.txt", Follow::Always)
        .expect("follow");
    client_a.set_clock(clock).expect("clock");
    let first = client_a.sync(&location).expect("a publishes the note");
    let first_cursor = first.cursor.expect("first cursor");
    assert_eq!(first_cursor.device_id, DEVICE_A);
    assert_eq!(first_cursor.parent_id, None);
    assert_eq!(first_cursor.time_unix, clock);

    clock += 10;
    client_b.follow("notes/a.txt", Follow::See).expect("see");
    client_b.set_clock(clock).expect("clock");
    let seen = client_b.sync(&location).expect("b sees metadata");
    let seen_cursor = seen.cursor.expect("see cursor");
    assert_eq!(seen_cursor.device_id, DEVICE_B);
    assert_eq!(
        seen_cursor.parent_id.as_deref(),
        Some(first_cursor.snapshot_id.as_str())
    );
    assert!(
        !work_b.join("notes/a.txt").exists(),
        "see must not download"
    );
    let catalog = client_b.catalog(&location).expect("catalog");
    let note = catalog
        .iter()
        .find(|entry| entry.path == "notes/a.txt")
        .unwrap_or_else(|| panic!("catalog row missing from {catalog:?}"));
    assert_eq!(note.size, 3);
    assert_eq!(note.version_unix, first_cursor.time_unix);
    assert_eq!(note.devices, vec![DEVICE_A.to_string()]);
    assert_eq!(note.follow, Follow::See);
    assert!(!note.is_directory);

    clock += 10;
    client_b.follow("notes/a.txt", Follow::Once).expect("once");
    client_b.set_clock(clock).expect("clock");
    client_b.sync(&location).expect("b downloads once");
    assert_eq!(
        fs::read(work_b.join("notes/a.txt")).expect("once bytes"),
        b"one"
    );

    clock += 10;
    fs::write(work_a.join("notes/a.txt"), b"two").expect("edit");
    client_a.set_clock(clock).expect("clock");
    client_a.sync(&location).expect("a publishes two");
    clock += 10;
    client_b.set_clock(clock).expect("clock");
    client_b
        .sync(&location)
        .expect("b keeps the first download");
    assert_eq!(
        fs::read(work_b.join("notes/a.txt")).expect("pinned bytes"),
        b"one",
        "baixar agora does not follow a later edit"
    );

    clock += 10;
    fs::create_dir_all(work_a.join("docs")).expect("docs");
    fs::write(work_a.join("docs/a.txt"), b"a1").expect("doc");
    symlink("a.txt", work_a.join("docs/link")).expect("symlink");
    client_a.follow("docs", Follow::Always).expect("folder");
    client_a.set_clock(clock).expect("clock");
    client_a.sync(&location).expect("a publishes docs");

    clock += 10;
    client_b.follow("docs", Follow::Always).expect("folder");
    client_b.set_clock(clock).expect("clock");
    client_b.sync(&location).expect("b follows the folder");
    assert_eq!(fs::read(work_b.join("docs/a.txt")).expect("a1"), b"a1");
    assert!(
        !work_b.join("docs/link").exists(),
        "a symlink is not a v1 file"
    );

    clock += 10;
    fs::write(work_b.join("docs/b.txt"), b"b").expect("new child");
    client_b.set_clock(clock).expect("clock");
    client_b.sync(&location).expect("b uploads the new child");
    clock += 10;
    client_a.set_clock(clock).expect("clock");
    client_a.sync(&location).expect("a receives the new child");
    assert_eq!(fs::read(work_a.join("docs/b.txt")).expect("child"), b"b");

    clock += 10;
    fs::write(work_a.join("docs/a.txt"), b"a2").expect("a2");
    client_a.set_clock(clock).expect("clock");
    let edited = client_a.sync(&location).expect("a publishes a2");
    let before_delete = edited.cursor.expect("a2 cursor").snapshot_id;
    clock += 10;
    client_b.set_clock(clock).expect("clock");
    client_b.sync(&location).expect("b follows a2");
    assert_eq!(fs::read(work_b.join("docs/a.txt")).expect("a2"), b"a2");

    let restored_work = workspace.path().join("a-restored");
    let mut restored = Client::open(device_a, &restored_work, PASSWORD).expect("reinstall");
    clock += 10;
    restored.set_clock(clock).expect("clock");
    restored.sync(&location).expect("reinstall sync");
    assert_eq!(
        fs::read(restored_work.join("docs/a.txt")).expect("restored"),
        b"a2",
        "a new process restores from the snapshot cursor"
    );
    drop(restored);

    clock += 10;
    fs::remove_file(work_a.join("docs/a.txt")).expect("delete");
    client_a.set_clock(clock).expect("clock");
    client_a.sync(&location).expect("a tombstone");
    clock += 10;
    client_b.set_clock(clock).expect("clock");
    client_b.sync(&location).expect("b drops the working copy");
    assert!(
        !work_b.join("docs/a.txt").exists(),
        "delete removes the working copy"
    );
    clock += 10;
    client_b.set_clock(clock).expect("clock");
    client_b.sync(&location).expect("b does not resurrect");
    clock += 10;
    client_a.set_clock(clock).expect("clock");
    client_a.sync(&location).expect("a stays deleted");
    assert!(!work_a.join("docs/a.txt").exists());
    assert!(!work_b.join("docs/a.txt").exists());
    assert_eq!(
        read_version(&location, PASSWORD, &before_delete, "docs/a.txt").expect("history"),
        b"a2",
        "history keeps the deleted bytes"
    );

    clock += 10;
    fs::write(work_a.join("docs/b.txt"), b"from-a").expect("conflict a");
    fs::write(work_b.join("docs/b.txt"), b"from-b").expect("conflict b");
    client_a.set_clock(clock).expect("clock");
    client_a.sync(&location).expect("a wins the primary");
    clock += 10;
    client_b.set_clock(clock).expect("clock");
    let conflicted = client_b
        .sync(&location)
        .expect("b uploads a marked version");
    assert_eq!(
        fs::read(work_a.join("docs/b.txt")).expect("primary a"),
        b"from-a"
    );
    assert_eq!(
        fs::read(work_b.join("docs/b.txt")).expect("primary b"),
        b"from-a"
    );
    assert_eq!(conflicted.conflicts.len(), 1, "one marked version");
    let marked = &conflicted.conflicts[0];
    assert_eq!(marked.path, "docs/b.txt");
    assert_ne!(marked.marked_path, "docs/b.txt");
    assert!(!work_a.join(&marked.marked_path).exists());
    assert!(!work_b.join(Path::new(&marked.marked_path)).exists());
    assert_eq!(
        read_version(
            &location,
            PASSWORD,
            &marked.snapshot_id,
            &marked.marked_path
        )
        .expect("mark"),
        b"from-b"
    );

    clock += 10;
    client_a.set_clock(clock).expect("clock");
    let again_a = client_a.sync(&location).expect("a does not requeue");
    clock += 10;
    client_b.set_clock(clock).expect("clock");
    let again_b = client_b.sync(&location).expect("b does not requeue");
    assert!(again_a.conflicts.is_empty());
    assert!(again_b.conflicts.is_empty());
    assert_eq!(
        fs::read(work_a.join("docs/b.txt")).expect("still primary"),
        b"from-a"
    );
    assert_eq!(client_b.conflicts(&location).expect("marks").len(), 1);

    let stored = cursors(&location, PASSWORD).expect("cursors");
    assert!(stored.iter().any(|cursor| {
        cursor.device_id == DEVICE_B
            && cursor.parent_id.as_deref() == Some(first_cursor.snapshot_id.as_str())
    }));
    assert!(
        stored
            .iter()
            .all(|cursor| { cursor.device_id == DEVICE_A || cursor.device_id == DEVICE_B })
    );
}
