//! Desktop shell: one folder, the OS keystore, and the add-device screen.
//!
//! The tray process is `satchel-desktop`. This library is what the tests drive
//! when no display is available.

mod app;
mod error;
mod picker;
mod profile;
mod secrets;
mod setup_ui;
mod text;

pub use app::{
    AccessPair, CreatedProfile, Desktop, Granted, JoinTarget, PublishReport, TrayCommand, TrayItem,
    TrayView, add_device_config, default_config_dir, phone_grant_secret, request_grant,
    tray_mark_rgba,
};
pub use error::Error;
pub use picker::{FolderPicker, PickFor, native_picker};
pub use profile::{ObjectStore, Profile, Role};
pub use secrets::{Account, KeyringStore, MemoryStore, SecretStore};
pub use setup_ui::{
    SetupPage, start as start_setup_page, start_with_picker as start_setup_page_with_picker,
};
pub use text::{Language, describe};
