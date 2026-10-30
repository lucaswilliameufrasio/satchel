//! Create and write a restic repository without putting the machine name in the key file.

mod error;
mod format;

pub use error::Error;
pub use format::{DeviceId, Location, S3Store, backup_directory, init};
