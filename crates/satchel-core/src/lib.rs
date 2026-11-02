//! Create and write a restic repository without putting the machine name in the key file.

mod credential;
mod error;
mod format;
mod probe;
mod setup;
mod storage;
mod sync;

pub use credential::{phone_credential_policy, phone_may_delete};
pub use error::Error;
pub use format::{
    DeviceId, DeviceKeyRecord, Location, S3Store, add_device_key, backup_directory, init,
    list_other_device_keys, remove_device_key,
};
pub use probe::{ObjectProtection, probe_object_protection};
pub use setup::{
    CreatedRepository, StorageSetup, check_storage, create_repository, generate_recovery_password,
};
pub use storage::{SetupCheck, StorageKind, Versioning, check_setup};
pub use sync::{
    CatalogEntry, Client, Cursor, DeviceCard, Follow, MarkedConflict, SyncOutcome, cursors,
    publish_folder, read_followed, read_version, restore_latest, sync_folder,
};
