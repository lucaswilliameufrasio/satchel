//! Setup gate for storage that can roll back objects.

use crate::Error;

/// Kind of storage the repository uses.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StorageKind {
    /// A local directory. It has no object versioning.
    Directory,
    /// Amazon S3 or another S3-compatible API that reports versioning.
    S3,
    /// Cloudflare R2.
    R2,
    /// Backblaze B2.
    B2,
    /// WebDAV. The setup can only warn.
    WebDav,
    /// SFTP. The setup can only warn.
    Sftp,
}

/// Bucket versioning as reported by the storage API.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Versioning {
    /// Versioning is on.
    Enabled,
    /// Versioning was on and is now suspended.
    Suspended,
    /// Versioning is off.
    Off,
}

/// Result of the setup gate when it does not refuse.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SetupCheck {
    /// Object versioning is on and lifecycle rules do not delete objects.
    Ready,
    /// This storage cannot prove that an overwritten object is still recoverable.
    Unprotected,
}

/// Refuse S3, R2, and B2 unless versioning is enabled and lifecycle does not delete objects.
///
/// Directory, WebDAV, and SFTP cannot offer that protection, so the setup continues with a warning.
/// The caller supplies the probe. This function does not contact the network.
pub fn check_setup(
    kind: StorageKind,
    versioning: Option<Versioning>,
    lifecycle_deletes_objects: bool,
) -> Result<SetupCheck, Error> {
    match kind {
        StorageKind::Directory | StorageKind::WebDav | StorageKind::Sftp => {
            Ok(SetupCheck::Unprotected)
        }
        StorageKind::S3 | StorageKind::R2 | StorageKind::B2 => {
            if versioning == Some(Versioning::Enabled) && !lifecycle_deletes_objects {
                Ok(SetupCheck::Ready)
            } else {
                Err(Error::ObjectProtection)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn object_storage_requires_versioning_without_a_deleting_lifecycle() {
        for kind in [StorageKind::S3, StorageKind::R2, StorageKind::B2] {
            let ready = check_setup(kind, Some(Versioning::Enabled), false).expect("ready");
            assert_eq!(ready, SetupCheck::Ready);
            assert!(check_setup(kind, Some(Versioning::Off), false).is_err());
            assert!(check_setup(kind, Some(Versioning::Suspended), false).is_err());
            assert!(check_setup(kind, None, false).is_err());
            assert!(check_setup(kind, Some(Versioning::Enabled), true).is_err());
        }
    }

    #[test]
    fn local_and_file_protocols_warn_that_versioning_is_absent() {
        for kind in [
            StorageKind::Directory,
            StorageKind::WebDav,
            StorageKind::Sftp,
        ] {
            let check = check_setup(kind, None, false).expect("warning");
            assert_eq!(check, SetupCheck::Unprotected);
        }
    }
}
