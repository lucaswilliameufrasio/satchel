//! Desktop shell: one folder, the OS keystore, and the add-device screen.
//!
//! The tray process is `satchel-desktop`. This library is what the tests drive
//! when no display is available.

mod app;
mod error;
mod profile;
mod secrets;
mod setup_ui;
mod wake;

pub use app::{
    AccessPair, CreatedProfile, Desktop, Granted, JoinTarget, PublishReport, TrayCommand, TrayItem,
    TrayView, default_config_dir, phone_grant_secret, request_grant, tray_mark_rgba,
};
pub use error::Error;
pub use profile::{ObjectStore, Profile, Role};
pub use secrets::{Account, KeyringStore, MemoryStore, SecretStore};
pub use setup_ui::{SetupPage, start as start_setup_page};
