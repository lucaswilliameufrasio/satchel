use std::path::PathBuf;

/// Failure from the repository core.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The device id is not 32 lowercase hexadecimal characters.
    #[error("device id must be 32 lowercase hexadecimal characters")]
    DeviceId,

    /// The recovery password is empty.
    #[error("recovery password is empty")]
    EmptyPassword,

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
