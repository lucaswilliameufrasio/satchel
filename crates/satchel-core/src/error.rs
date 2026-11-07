use std::path::PathBuf;

/// Failure from the repository core.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A wake key is not 32 bytes of hex, or a wake socket could not be opened.
    #[error("wake notice setup failed")]
    Wake,

    /// The device id is not 32 lowercase hexadecimal characters.
    #[error("device id must be 32 lowercase hexadecimal characters")]
    DeviceId,

    /// The recovery password is empty.
    #[error("recovery password is empty")]
    EmptyPassword,

    /// A key id is not 64 lowercase hexadecimal characters.
    #[error("key id must be 64 lowercase hexadecimal characters")]
    KeyId,

    /// Removal targeted the key that opened the repository.
    #[error("the open key cannot be removed")]
    CurrentKey,

    /// A required storage field is empty.
    #[error("storage field `{field}` is empty")]
    EmptyField {
        /// Name of the missing field.
        field: &'static str,
    },

    /// The directory to back up does not exist or is not a directory.
    #[error("source `{path}` is not a directory")]
    SourceDirectory {
        /// Path that was supposed to be a directory.
        path: PathBuf,
    },

    /// A catalog path is absolute, empty, or walks upward.
    #[error("path `{path}` is not a relative file path")]
    Path {
        /// The rejected path.
        path: String,
    },

    /// The controlled clock is negative.
    #[error("clock must be zero or later")]
    Clock,

    /// A device label is empty, longer than 64 characters, or has a control character.
    #[error("device label must be 1 to 64 characters without control characters")]
    Label,

    /// S3, R2, or B2 is missing versioning, or a lifecycle rule deletes objects.
    #[error("object storage needs versioning enabled and no lifecycle rule that deletes objects")]
    ObjectProtection,

    /// The new password is the one already in use.
    #[error("the new password is the same as the current one")]
    SamePassword,

    /// The phone folder listed no file while the last round saw some.
    ///
    /// A revoked folder permission or an unmounted card looks like this. Nothing
    /// is deleted on either side until the folder is reachable again.
    #[error("the folder looks empty or unreachable, so nothing was deleted")]
    FolderUnavailable,

    /// A repository already exists at this location.
    #[error("a repository already exists here")]
    RepositoryExists,

    /// WebDAV and SFTP have no setup in this cut.
    #[error("this storage kind has no setup yet")]
    UnsupportedStorage,

    /// The storage refused the key when it read the bucket's settings (HTTP 401 or 403).
    #[error(
        "the storage would not let this key read the bucket's versioning and lifecycle settings"
    )]
    ProbeDenied,

    /// The bucket probe did not return a usable answer. The message has no credential.
    #[error("object storage probe failed")]
    Probe,

    /// A snapshot does not contain the requested file.
    #[error("snapshot `{snapshot}` has no file `{path}`")]
    MissingFile {
        /// Snapshot id that was read.
        snapshot: String,
        /// Relative path that was missing.
        path: String,
    },

    /// rustic or the storage backend rejected the operation.
    #[error("{message}")]
    Repository {
        /// Display text from the underlying repository error.
        message: String,
    },
}

impl Error {
    pub(crate) fn repository(err: impl std::fmt::Display) -> Self {
        Self::Repository {
            message: err.to_string(),
        }
    }
}
