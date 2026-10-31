//! A phone mark is a new snapshot, not a session-only flag.

use std::fs;

use satchel_core::{Client, DeviceId, Follow, Location, init};
use tempfile::tempdir;

const DESKTOP: &str = "0123456789abcdef0123456789abcdef";
const PHONE: &str = "fedcba9876543210fedcba9876543210";
const PASSWORD: &str = "recovery-password";

#[test]
fn phone_mark_reopens_from_a_new_snapshot() {
    let workspace = tempdir().expect("temp dir");
    let repository = workspace.path().join("repository");
    fs::create_dir_all(&repository).expect("repository");
    let location = Location::Directory(repository);
    let desktop_id = DeviceId::parse(DESKTOP).expect("desktop id");
    let phone_id = DeviceId::parse(PHONE).expect("phone id");
    init(&location, PASSWORD, &desktop_id).expect("init");

    let desktop_work = workspace.path().join("desktop");
    fs::create_dir_all(desktop_work.join("notes")).expect("notes");
    fs::write(desktop_work.join("notes/kept.txt"), b"desk-bytes").expect("file");
    let mut desktop = Client::open(desktop_id, &desktop_work, PASSWORD).expect("desktop");
    desktop
        .follow("notes/kept.txt", Follow::Always)
        .expect("desktop follow");
    desktop.set_clock(1_700_000_000).expect("desktop clock");
    let published = desktop.sync(&location).expect("desktop sync");
    let desktop_cursor = published.cursor.expect("desktop cursor");

    let phone_work = workspace.path().join("phone");
    let mut phone = Client::open(phone_id.clone(), &phone_work, PASSWORD).expect("phone");
    let first = phone.catalog(&location).expect("first catalog");
    let unseen = first
        .iter()
        .find(|entry| entry.path == "notes/kept.txt")
        .expect("metadata");
    assert_eq!(unseen.follow, Follow::See);
    assert_eq!(unseen.devices, vec![DESKTOP.to_string()]);
    assert!(
        !phone_work.join("notes/kept.txt").exists(),
        "first look wrote the work copy"
    );

    phone.follow("notes/kept.txt", Follow::Once).expect("once");
    phone.set_clock(1_700_000_010).expect("once clock");
    phone.sync(&location).expect("once sync");
    assert_eq!(
        fs::read(phone_work.join("notes/kept.txt")).expect("once bytes"),
        b"desk-bytes"
    );
    let once_cursor = latest_cursor(&location, PHONE);
    assert_ne!(once_cursor.snapshot_id, desktop_cursor.snapshot_id);
    assert_eq!(once_cursor.snapshot_id.len(), 64);
    assert_eq!(
        once_cursor.parent_id.as_deref(),
        Some(desktop_cursor.snapshot_id.as_str())
    );
    drop(phone);

    let reopened = Client::open(phone_id.clone(), &phone_work, PASSWORD).expect("reopen");
    let marked = reopened.catalog(&location).expect("reopened catalog");
    let once_row = marked
        .iter()
        .find(|entry| entry.path == "notes/kept.txt")
        .expect("once row");
    assert_eq!(once_row.follow, Follow::Once);
    assert_eq!(
        once_row.devices,
        vec![DESKTOP.to_string(), PHONE.to_string()]
    );
    let desktop_view = desktop.catalog(&location).expect("desktop catalog");
    let shared = desktop_view
        .iter()
        .find(|entry| entry.path == "notes/kept.txt")
        .expect("desktop row");
    assert_eq!(shared.devices, vec![DESKTOP.to_string(), PHONE.to_string()]);
    drop(reopened);

    let mut clearing = Client::open(phone_id, &phone_work, PASSWORD).expect("clear");
    clearing.follow("notes/kept.txt", Follow::See).expect("see");
    clearing.set_clock(1_700_000_020).expect("see clock");
    clearing.sync(&location).expect("see sync");
    drop(clearing);

    let seen = Client::open(
        DeviceId::parse(PHONE).expect("phone again"),
        &phone_work,
        PASSWORD,
    )
    .expect("seen");
    let see_row = seen
        .catalog(&location)
        .expect("see catalog")
        .into_iter()
        .find(|entry| entry.path == "notes/kept.txt")
        .expect("see row");
    assert_eq!(see_row.follow, Follow::See);
    assert_eq!(see_row.devices, vec![DESKTOP.to_string()]);
    assert_eq!(
        fs::read(phone_work.join("notes/kept.txt")).expect("kept download"),
        b"desk-bytes"
    );
}

fn latest_cursor(location: &Location, device: &str) -> satchel_core::Cursor {
    let listed = satchel_core::cursors(location, PASSWORD).expect("cursors");
    let mut chosen: Option<satchel_core::Cursor> = None;
    for cursor in listed {
        if cursor.device_id != device {
            continue;
        }
        let replace = match &chosen {
            None => true,
            Some(current) => cursor.time_unix > current.time_unix,
        };
        if replace {
            chosen = Some(cursor);
        }
    }
    chosen.expect("cursor")
}
