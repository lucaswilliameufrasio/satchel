//! Create and write a restic repository without putting the machine name in the key file.

mod error;
mod format;
mod storage;
mod sync;

pub use error::Error;
pub use format::{
    DeviceId, Location, S3Store, add_device_key, backup_directory, init, remove_device_key,
};
pub use storage::{SetupCheck, StorageKind, Versioning, check_setup};
pub use sync::{
    CatalogEntry, Client, Cursor, Follow, MarkedConflict, SyncOutcome, cursors, read_version,
    restore_latest,
};
