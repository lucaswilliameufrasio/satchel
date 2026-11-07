//! Several folders on one desktop: add, receive, stop, a disk that goes away, and leaving.

use std::fs;

use satchel_core::{Lane, Location, StorageSetup, folders, sync_folder_in};
use satchel_desktop::{Account, Desktop, Language, MemoryStore, SecretStore};
use tempfile::tempdir;

#[test]
fn a_profile_from_before_folders_keeps_its_one_folder() {
    let root = tempdir().expect("temp");
    let config = root.path().join("config");
    fs::create_dir_all(&config).expect("config");
    let folder = root.path().join("Documentos");
    fs::write(
        config.join("profile.json"),
        serde_json::json!({
            "folder": folder.to_str().expect("path"),
            "repository": root.path().join("repo").to_str().expect("path"),
            "device_id": "0123456789abcdef0123456789abcdef",
            "storage": "directory",
            "role": "maintainer",
        })
        .to_string(),
    )
    .expect("old profile");
    let desktop = Desktop::open(&config).expect("open");
    let profile = desktop.profile().expect("profile");
    assert_eq!(profile.folders.len(), 1);
    assert_eq!(
        profile.folders[0].lane, None,
        "the repository's first folder"
    );
    assert_eq!(profile.folders[0].name, "Documentos");
    assert_eq!(profile.folders[0].path, folder);
}

#[test]
fn folders_are_added_received_stopped_and_paused_without_deleting() {
    let root = tempdir().expect("temp");
    let docs = root.path().join("Documentos");
    let photos = root.path().join("Fotos");
    let repository = root.path().join("copies");
    fs::create_dir_all(&docs).expect("docs");
    fs::create_dir_all(&photos).expect("photos");
    fs::write(docs.join("luz.txt"), b"conta").expect("doc");
    fs::write(photos.join("praia.jpg"), b"foto").expect("photo");

    let store = MemoryStore::default();
    let mut desktop = Desktop::open(root.path().join("config"))
        .expect("open")
        .with_language(Language::En);
    desktop
        .create_repository(
            &store,
            docs.clone(),
            StorageSetup::directory(repository.clone()),
            None,
        )
        .expect("create");
    desktop.add_folder(photos.clone(), "").expect("add photos");
    // Overlapping folders are refused, and so is the same folder twice.
    assert!(matches!(
        desktop.add_folder(docs.join("inside"), ""),
        Err(satchel_desktop::Error::Path { .. } | satchel_desktop::Error::Nested { .. })
    ));
    fs::create_dir_all(docs.join("inside")).expect("inside");
    assert!(matches!(
        desktop.add_folder(docs.join("inside"), ""),
        Err(satchel_desktop::Error::Nested { .. })
    ));
    desktop.publish(&store).expect("sync both");

    let profile = desktop.profile().expect("profile").clone();
    assert_eq!(profile.folders.len(), 2);
    assert_eq!(profile.folders[0].lane, None, "the first stays the first");
    let photos_lane = profile.folders[1].lane.clone().expect("an id");
    assert_eq!(profile.folders[1].name, "Fotos");
    let listed = desktop.repository_folders();
    assert_eq!(listed.len(), 2, "{listed:?}");
    assert!(listed.iter().any(|f| f.name.as_deref() == Some("Fotos")));

    // Another device publishes a third folder; this desktop sees it and receives it.
    let password = store
        .get(Account::RecoveryPassword)
        .expect("get")
        .expect("password");
    let location = Location::Directory(repository.clone());
    let other = satchel_core::DeviceId::parse("fedcba9876543210fedcba9876543210").expect("id");
    let music_there = root.path().join("there/Musica");
    fs::create_dir_all(&music_there).expect("music");
    fs::write(music_there.join("faixa.txt"), b"la la").expect("track");
    let music = Lane::named("4d75736963", "Música").expect("lane");
    sync_folder_in(&location, &password, &other, &music_there, &music, None).expect("other");
    desktop.refresh_folders(&store).expect("refresh");
    assert!(
        desktop
            .repository_folders()
            .iter()
            .any(|f| f.name.as_deref() == Some("Música"))
    );
    let music_here = root.path().join("Musica aqui");
    fs::create_dir_all(&music_here).expect("here");
    desktop
        .receive_folder(music.id.clone(), "Música", music_here.clone())
        .expect("receive");
    desktop.publish(&store).expect("sync");
    assert_eq!(
        fs::read(music_here.join("faixa.txt")).expect("track"),
        b"la la"
    );
    assert!(!music_here.join("luz.txt").exists(), "folders do not mix");

    // The disk with the photos goes away: that folder pauses and nothing is deleted.
    let away = root.path().join("Fotos-away");
    fs::rename(&photos, &away).expect("unplug");
    desktop.publish(&store).expect("sync without the photos");
    assert!(
        desktop
            .unreachable_folders()
            .contains(&Some(photos_lane.clone()))
    );
    let view = desktop.tray_view();
    assert!(
        view.items
            .iter()
            .any(|item| item.label.contains("isn't connected, paused")),
        "{:?}",
        view.items
            .iter()
            .map(|item| &item.label)
            .collect::<Vec<_>>()
    );
    let reader = root.path().join("reader");
    fs::create_dir_all(&reader).expect("reader");
    let photos_lane_ref = Lane::named(&photos_lane, "Fotos").expect("lane");
    sync_folder_in(
        &location,
        &password,
        &other,
        &reader,
        &photos_lane_ref,
        None,
    )
    .expect("read");
    assert!(
        reader.join("praia.jpg").exists(),
        "the photo is still in the repository"
    );
    fs::rename(&away, &photos).expect("plug back");
    desktop.publish(&store).expect("sync again");
    assert!(desktop.unreachable_folders().is_empty());

    // Stopping a folder keeps its files here and in the repository.
    desktop.stop_folder(Some(&photos_lane)).expect("stop");
    assert!(photos.join("praia.jpg").exists());
    desktop.publish(&store).expect("sync after stop");
    assert!(
        folders(&location, &password)
            .expect("folders")
            .iter()
            .any(|f| f.id.as_deref() == Some(photos_lane.as_str()))
    );
    assert_eq!(desktop.profile().expect("profile").folders.len(), 2);

    // Disconnecting forgets the settings and the keys, and nothing else.
    desktop.disconnect(&store).expect("disconnect");
    assert!(desktop.profile().is_none());
    assert!(store.get(Account::RecoveryPassword).expect("get").is_none());
    assert!(docs.join("luz.txt").exists());
    assert!(repository.join("config").exists());
    let reopened = Desktop::open(root.path().join("config")).expect("reopen");
    assert!(reopened.profile().is_none());
}

#[test]
fn a_disconnected_copies_disk_pauses_the_whole_sync() {
    let root = tempdir().expect("temp");
    let docs = root.path().join("Documentos");
    let repository = root.path().join("copies");
    fs::create_dir_all(&docs).expect("docs");
    let store = MemoryStore::default();
    let mut desktop = Desktop::open(root.path().join("config"))
        .expect("open")
        .with_language(Language::En);
    desktop
        .create_repository(
            &store,
            docs,
            StorageSetup::directory(repository.clone()),
            None,
        )
        .expect("create");
    fs::rename(&repository, root.path().join("copies-away")).expect("unplug");
    let err = desktop.publish(&store).expect_err("paused");
    assert!(matches!(err, satchel_desktop::Error::Unreachable { .. }));
    assert!(
        !repository.exists(),
        "an empty repository was not made in its place"
    );
    let view = desktop.tray_view();
    assert!(
        view.items
            .iter()
            .any(|item| item.label.contains("encrypted copies isn't connected")),
        "{:?}",
        view.items
            .iter()
            .map(|item| &item.label)
            .collect::<Vec<_>>()
    );
}
