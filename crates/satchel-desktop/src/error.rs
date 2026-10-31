use std::path::PathBuf;

/// Failure from the desktop shell. Messages do not include secret values.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A profile is already stored in this config directory.
    #[error("a folder is already configured")]
    ProfileExists,

    /// The folder or the repository path cannot be used.
    #[error("path `{path}` is not usable")]
    Path {
        /// Path that was rejected.
        path: PathBuf,
    },

    /// The profile file is missing a field or is not valid JSON.
    #[error("the profile file is invalid")]
    Profile,

    /// The add-device screen is already open.
    #[error("pairing is already open")]
    AlreadyPairing,

    /// Approve was asked for and the keystore has no desktop bucket credential.
    #[error("the bucket credential is not in the keystore")]
    MissingSecret,

    /// Approve was asked for and the keystore has no phone bucket credential.
    #[error("the phone credential is not in the keystore")]
    MissingPhoneCredential,

    /// The phone credential cannot be sent as a bucket grant.
    #[error("the phone credential is not usable")]
    PhoneCredential,

    /// Approve was asked for and the keystore has no recovery password.
    #[error("the recovery password is not in the keystore")]
    MissingRecovery,

    /// The OS keystore rejected the call. `kind` is a category, not the stored value.
    #[error("keystore {kind}")]
    Store {
        /// Coarse reason such as `locked` or `platform`.
        kind: &'static str,
    },

    /// StatusNotifier did not start.
    #[error("the tray did not start")]
    Tray,

    /// Filesystem failure. The message is the error kind, not file contents.
    #[error("filesystem error")]
    Io,

    /// Pairing failed.
    #[error(transparent)]
    Pair(#[from] satchel_pair::Error),

    /// The device id in the profile is not the opaque form.
    #[error(transparent)]
    Device(#[from] satchel_core::Error),
}
