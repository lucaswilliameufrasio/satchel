//! Desktop shell: one folder, the OS keystore, and the add-device screen.
//!
//! The tray process is `satchel-desktop`. This library is what the tests drive
//! when no display is available.

mod app;
mod error;
mod profile;
mod secrets;

pub use app::{Desktop, TrayCommand, TrayItem, TrayView, default_config_dir};
pub use error::Error;
pub use profile::Profile;
pub use secrets::{Account, KeyringStore, MemoryStore, SecretStore};
