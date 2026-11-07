//! Create and write a restic repository without putting the machine name in the key file.

mod credential;
mod error;
mod format;
mod mirror;
mod probe;
mod setup;
mod storage;
mod sync;
mod wake;

pub use credential::{phone_credential_policy, phone_may_delete};
pub use error::Error;
pub use format::{
    DeviceId, DeviceKeyRecord, Location, S3Store, add_device_key, backup_directory,
    change_recovery_password, init, list_other_device_keys, remove_device_key,
};
pub use mirror::{DirectoryTree, FolderTree, MirrorReport, TreeFile, sync_mirrored};
pub use probe::{ObjectProtection, probe_object_protection};
pub use setup::{
    CreatedRepository, ProvisionedKey, StorageSetup, check_storage, create_repository,
    generate_recovery_password, provision_device_key,
};
pub use storage::{SetupCheck, StorageKind, Versioning, check_setup};
pub use sync::{
    CatalogEntry, Client, Cursor, DeviceCard, FolderSummary, Follow, Lane, MarkedConflict,
    QuietMark, SyncOutcome, cursors, folders, publish_folder, quiet_mark, read_followed,
    read_followed_in, read_version, restore_latest, sync_folder, sync_folder_announcing,
    sync_folder_in, write_latest, write_latest_in,
};
pub use wake::{
    WAKE_WINDOW_SECS, WakeCheck, WakeEndpoint, WakeListener, WakePeer, bind_saved_wake,
    generate_wake_key, send_wakes, wake_datagram,
};
