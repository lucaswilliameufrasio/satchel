//! Several folders in one repository: each syncs as if it were alone.

use std::fs;

use satchel_core::{
    Client, DeviceId, Lane, Location, folders, init, sync_folder, sync_folder_in, write_latest,
    write_latest_in,
};
use tempfile::tempdir;

const DEVICE_A: &str = "0123456789abcdef0123456789abcdef";
const DEVICE_B: &str = "fedcba9876543210fedcba9876543210";
const PASSWORD: &str = "recovery-password";

#[test]
fn two_folders_share_a_repository_and_never_mix() {
    let root = tempdir().expect("temp");
    let repository = root.path().join("repository");
    fs::create_dir_all(&repository).expect("repository");
    let location = Location::Directory(repository);
    let device_a = DeviceId::parse(DEVICE_A).expect("a");
    let device_b = DeviceId::parse(DEVICE_B).expect("b");
    init(&location, PASSWORD, &device_a).expect("init");

    let docs_a = root.path().join("a/Documentos");
    let photos_a = root.path().join("a/Fotos");
    fs::create_dir_all(docs_a.join("contas")).expect("docs");
    fs::create_dir_all(&photos_a).expect("photos");
    fs::write(docs_a.join("contas/luz.txt"), b"conta de luz").expect("doc");
    fs::write(photos_a.join("praia.jpg"), b"not really a jpeg").expect("photo");

    let docs = Lane::first("Documentos").expect("first");
    let photos = Lane::named(&Lane::new_id(), "Fotos").expect("named");
    sync_folder_in(&location, PASSWORD, &device_a, &docs_a, &docs, None).expect("docs");
    sync_folder_in(&location, PASSWORD, &device_a, &photos_a, &photos, None).expect("photos");

    // The repository lists both, with the names people gave them.
    let listed = folders(&location, PASSWORD).expect("folders");
    assert_eq!(listed.len(), 2, "{listed:?}");
    let first = listed.iter().find(|f| f.id.is_none()).expect("first");
    assert_eq!(first.name.as_deref(), Some("Documentos"));
    assert_eq!(first.devices, vec![DEVICE_A.to_string()]);
    let second = listed.iter().find(|f| f.id == photos.id).expect("second");
    assert_eq!(second.name.as_deref(), Some("Fotos"));

    // Another device receives each folder in its own place, and nothing crosses.
    let docs_b = root.path().join("b/Docs");
    let photos_b = root.path().join("b/Fotos");
    fs::create_dir_all(&docs_b).expect("docs b");
    fs::create_dir_all(&photos_b).expect("photos b");
    sync_folder_in(&location, PASSWORD, &device_b, &photos_b, &photos, None).expect("photos b");
    assert_eq!(
        fs::read(photos_b.join("praia.jpg")).expect("photo"),
        b"not really a jpeg"
    );
    assert!(
        !photos_b.join("contas").exists(),
        "no document in the photos"
    );
    // A folder from before folders existed is the first one: the old call still works.
    sync_folder(&location, PASSWORD, &device_b, &docs_b).expect("docs b");
    assert_eq!(
        fs::read(docs_b.join("contas/luz.txt")).expect("doc"),
        b"conta de luz"
    );
    assert!(
        !docs_b.join("praia.jpg").exists(),
        "no photo in the documents"
    );

    // A change goes only to its folder.
    fs::write(photos_b.join("serra.jpg"), b"another one").expect("new photo");
    sync_folder_in(&location, PASSWORD, &device_b, &photos_b, &photos, None).expect("send");
    sync_folder_in(&location, PASSWORD, &device_a, &photos_a, &photos, None).expect("pull");
    sync_folder_in(&location, PASSWORD, &device_a, &docs_a, &docs, None).expect("docs again");
    assert!(photos_a.join("serra.jpg").exists());
    assert!(!docs_a.join("serra.jpg").exists());

    // Deleting every photo does not touch the documents.
    fs::remove_file(photos_a.join("praia.jpg")).expect("rm");
    fs::remove_file(photos_a.join("serra.jpg")).expect("rm");
    sync_folder_in(&location, PASSWORD, &device_a, &photos_a, &photos, None).expect("delete");
    sync_folder(&location, PASSWORD, &device_b, &docs_b).expect("docs b again");
    assert!(docs_b.join("contas/luz.txt").exists());

    // A reader that names the folder gets that folder's file, and only that one.
    let out = root.path().join("out.txt");
    write_latest(&location, PASSWORD, "contas/luz.txt", &out).expect("first folder");
    assert_eq!(fs::read(&out).expect("read"), b"conta de luz");
    assert!(
        write_latest_in(
            &location,
            PASSWORD,
            photos.id.as_deref(),
            "contas/luz.txt",
            &out
        )
        .is_err()
    );

    // The catalog of a client in one folder lists only that folder.
    let reader = Client::open(device_b.clone(), root.path().join("reader"), PASSWORD)
        .expect("reader")
        .in_lane(photos.clone());
    let catalog = reader.catalog(&location).expect("catalog");
    assert!(
        catalog
            .iter()
            .all(|entry| !entry.path.starts_with("contas")),
        "{catalog:?}"
    );
}

#[test]
fn a_folder_id_is_short_and_plain() {
    assert!(Lane::named("", "x").is_err());
    assert!(Lane::named("Fotos", "x").is_err());
    assert!(Lane::named("a/b", "x").is_err());
    assert!(Lane::named(&"a".repeat(65), "x").is_err());
    assert!(Lane::named("0f3a-9", "x").is_ok());
    assert!(Lane::first("").is_err());
    let id = Lane::new_id();
    assert_eq!(id.len(), 16);
    assert_ne!(id, Lane::new_id());
}

#[test]
fn devices_say_who_they_are_and_which_one_is_main() {
    use satchel_core::{Intro, devices, sync_folder_introduced};
    let root = tempdir().expect("temp");
    let repository = root.path().join("repository");
    fs::create_dir_all(&repository).expect("repository");
    let location = Location::Directory(repository);
    let device_a = DeviceId::parse(DEVICE_A).expect("a");
    let device_b = DeviceId::parse(DEVICE_B).expect("b");
    init(&location, PASSWORD, &device_a).expect("init");
    let docs = Lane::first("Documentos").expect("lane");
    let photos = Lane::named("f0705", "Fotos").expect("lane");
    let a_dir = root.path().join("a");
    let b_dir = root.path().join("b");
    fs::create_dir_all(&a_dir).expect("a");
    fs::create_dir_all(&b_dir).expect("b");
    let main = Intro {
        name: "MacBook de casa".to_string(),
        os: "macOS".to_string(),
        main: true,
    };
    sync_folder_introduced(&location, PASSWORD, &device_a, &a_dir, &docs, None, &main).expect("a");
    let other = Intro {
        name: "Notebook do trabalho".to_string(),
        os: "Linux".to_string(),
        main: false,
    };
    // Only in the second folder: it still counts as a device of the repository.
    sync_folder_introduced(
        &location, PASSWORD, &device_b, &b_dir, &photos, None, &other,
    )
    .expect("b");
    let cards = devices(&location, PASSWORD).expect("devices");
    assert_eq!(cards.len(), 2, "{cards:?}");
    let a = cards
        .iter()
        .find(|card| card.id == DEVICE_A)
        .expect("a card");
    assert_eq!(a.name.as_deref(), Some("MacBook de casa"));
    assert!(a.main);
    let b = cards
        .iter()
        .find(|card| card.id == DEVICE_B)
        .expect("b card");
    assert_eq!(b.os.as_deref(), Some("Linux"));
    assert!(!b.main);
}
