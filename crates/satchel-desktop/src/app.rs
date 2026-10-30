use std::path::{Path, PathBuf};

use rand::Rng;
use satchel_core::DeviceId;
use satchel_pair::{ApprovalRequest, Decision, DeviceSecrets, ListenConfig, Offer, PairingSession};

use crate::Error;
use crate::profile::{self, Profile};
use crate::secrets::{Account, SecretStore};

/// What one tray menu row does.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TrayCommand {
    /// Label only.
    None,
    /// Open the add-device screen.
    OpenAddDevice,
    /// Close the add-device screen and stop discovery.
    CloseAddDevice,
    /// Send the grant.
    Approve,
    /// Send nothing.
    Deny,
    /// Leave the app.
    Quit,
}

/// One row in the tray menu.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TrayItem {
    /// Text the menu shows.
    pub label: String,
    /// False for information rows.
    pub enabled: bool,
    /// What a click does.
    pub command: TrayCommand,
}

/// Menu contents derived from the desktop state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TrayView {
    /// Tooltip title.
    pub title: String,
    /// Rows from top to bottom.
    pub items: Vec<TrayItem>,
    /// Invite JSON while the add-device screen is open, before approval.
    pub invite: Option<String>,
}

/// One configured folder, the add-device session, and the approval on screen.
pub struct Desktop {
    config_dir: PathBuf,
    profile: Option<Profile>,
    session: Option<PairingSession>,
    pending: Option<ApprovalRequest>,
}

impl Desktop {
    /// Load a config directory. The directory is created if it is missing.
    pub fn open(config_dir: impl Into<PathBuf>) -> Result<Self, Error> {
        let config_dir = config_dir.into();
        std::fs::create_dir_all(&config_dir).map_err(|_| Error::Io)?;
        let profile = profile::load(&config_dir)?;
        Ok(Self {
            config_dir,
            profile,
            session: None,
            pending: None,
        })
    }

    /// The configured folder, when setup has been done.
    pub fn profile(&self) -> Option<&Profile> {
        self.profile.as_ref()
    }

    /// Record the one folder. A second call fails.
    pub fn create_profile(
        &mut self,
        folder: PathBuf,
        repository: PathBuf,
        device_id: DeviceId,
    ) -> Result<(), Error> {
        if self.profile.is_some() {
            return Err(Error::ProfileExists);
        }
        let profile = profile::create(&self.config_dir, folder, repository, device_id)?;
        self.profile = Some(profile);
        Ok(())
    }

    /// Start listening. Discovery follows `config.advertise` and ends when the session is dropped.
    pub fn open_add_device(&mut self, config: ListenConfig) -> Result<Offer, Error> {
        if self.session.is_some() {
            return Err(Error::AlreadyPairing);
        }
        let session = PairingSession::listen(config)?;
        let offer = session.offer().clone();
        self.session = Some(session);
        self.pending = None;
        Ok(offer)
    }

    /// True while the add-device screen owns a session.
    pub fn add_device_open(&self) -> bool {
        self.session.is_some()
    }

    /// Stop listening and stop announcing.
    pub fn close_add_device(&mut self) {
        self.pending = None;
        self.session = None;
    }

    /// Pull an approval in, if the joiner has passed the code check.
    pub fn poll(&mut self) {
        let Some(session) = self.session.as_ref() else {
            return;
        };
        if self.pending.is_some() {
            return;
        }
        if let Ok(Some(request)) = session.try_approval() {
            self.pending = Some(request);
        }
    }

    /// The approval on screen, if the joiner has already passed the code check.
    pub fn pending(&self) -> Option<&ApprovalRequest> {
        self.pending.as_ref()
    }

    /// Send the endpoint, the bucket credential, and a new device key password.
    ///
    /// The recovery password is not read.
    pub fn approve_pending(&mut self, store: &dyn SecretStore) -> Result<(), Error> {
        if self.pending.is_none() {
            return Err(satchel_pair::Error::Closed.into());
        }
        let Some(profile) = self.profile.clone() else {
            return Err(Error::Profile);
        };
        let Some(bucket_credential) = store.get(Account::BucketCredential)? else {
            return Err(Error::MissingSecret);
        };
        let secrets = DeviceSecrets {
            endpoint: path_endpoint(&profile.repository),
            bucket_credential,
            device_key_password: new_device_password(),
        };
        self.finish(Decision::Approve(secrets))
    }

    /// Close the attempt without a grant.
    pub fn deny_pending(&mut self) -> Result<(), Error> {
        if self.pending.is_none() {
            return Err(satchel_pair::Error::Closed.into());
        }
        self.finish(Decision::Deny)
    }

    /// Rows for the tray. Labels include the approval fields and not the keystore values.
    pub fn tray_view(&self) -> TrayView {
        let mut items = Vec::new();
        if let Some(profile) = &self.profile {
            items.push(item(
                format!("Folder {}", profile.folder.display()),
                false,
                TrayCommand::None,
            ));
        } else {
            items.push(item("No folder".to_string(), false, TrayCommand::None));
        }
        let mut invite = None;
        if let Some(pending) = &self.pending {
            items.push(item(
                format!("Device {}", pending.device_name),
                false,
                TrayCommand::None,
            ));
            items.push(item(
                format!("System {}", pending.os),
                false,
                TrayCommand::None,
            ));
            items.push(item(
                format!("Address {}", pending.peer_address),
                false,
                TrayCommand::None,
            ));
            items.push(item(
                format!("Interface {}", pending.local_interface),
                false,
                TrayCommand::None,
            ));
            items.push(item(
                format!("Code {}", pending.code),
                false,
                TrayCommand::None,
            ));
            items.push(item("Approve".to_string(), true, TrayCommand::Approve));
            items.push(item("Deny".to_string(), true, TrayCommand::Deny));
        } else if let Some(session) = &self.session {
            let offer = session.offer();
            invite = Some(offer.invite.clone());
            items.push(item(
                format!("Code {}", offer.code),
                false,
                TrayCommand::None,
            ));
            items.push(item(
                format!("Address {}", offer.address),
                false,
                TrayCommand::None,
            ));
            items.push(item(
                format!("Fingerprint {}", offer.fingerprint),
                false,
                TrayCommand::None,
            ));
            items.push(item(
                "Cancel".to_string(),
                true,
                TrayCommand::CloseAddDevice,
            ));
        } else if self.profile.is_some() {
            items.push(item(
                "Add device".to_string(),
                true,
                TrayCommand::OpenAddDevice,
            ));
        }
        items.push(item("Quit".to_string(), true, TrayCommand::Quit));
        TrayView {
            title: "Satchel".to_string(),
            items,
            invite,
        }
    }

    fn finish(&mut self, decision: Decision) -> Result<(), Error> {
        let Some(session) = self.session.as_ref() else {
            return Err(satchel_pair::Error::Closed.into());
        };
        session.decide(decision)?;
        self.pending = None;
        self.session = None;
        Ok(())
    }
}

/// Config directory for the installed app.
pub fn default_config_dir() -> PathBuf {
    if let Some(config) = std::env::var_os("XDG_CONFIG_HOME") {
        return PathBuf::from(config).join("satchel");
    }
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home).join(".config").join("satchel");
    }
    PathBuf::from(".config").join("satchel")
}

fn item(label: String, enabled: bool, command: TrayCommand) -> TrayItem {
    TrayItem {
        label,
        enabled,
        command,
    }
}

fn path_endpoint(path: &Path) -> String {
    path.display().to_string()
}

fn new_device_password() -> String {
    let mut bytes = [0_u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        text.push_str(&format!("{byte:02x}"));
    }
    text
}
