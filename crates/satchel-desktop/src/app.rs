use std::path::PathBuf;

use rand::Rng;
use satchel_core::{
    DeviceId, DeviceKeyRecord, Location, S3Store, SetupCheck, StorageKind, add_device_key,
    check_setup, list_other_device_keys, probe_object_protection, publish_folder,
    remove_device_key,
};
use satchel_pair::{
    ApprovalRequest, Decision, DeviceSecrets, Invite, ListenConfig, Offer, PairingSession,
};
use serde::Deserialize;

use crate::Error;
use crate::profile::{self, ObjectStore, Profile};
use crate::secrets::{Account, SecretStore};

/// What one tray menu row does.
#[derive(Clone, Debug, Eq, PartialEq)]
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
    /// Publish the folder into the repository.
    Publish,
    /// Read device keys for the revoke rows.
    LoadDevices,
    /// Delete one device key. The id is 64 hex characters and is not a secret.
    Revoke(String),
    /// Leave the app.
    Quit,
}

/// What one publish wrote, and whether object versioning protects it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublishReport {
    /// Snapshot cursor after the publish.
    pub outcome: satchel_core::SyncOutcome,
    /// Ready for object storage, or a warning for a local folder.
    pub protection: SetupCheck,
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
    /// PNG shown on the row. Used for the invite QR.
    pub icon_png: Option<Vec<u8>>,
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
    device_keys: Option<Vec<DeviceKeyRecord>>,
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
            device_keys: None,
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

    /// Remember an S3-compatible bucket. The access keys stay in the keystore.
    pub fn attach_object_store(&mut self, object_store: ObjectStore) -> Result<(), Error> {
        let Some(profile) = self.profile.clone() else {
            return Err(Error::Profile);
        };
        let updated = profile::attach_object_store(&self.config_dir, &profile, object_store)?;
        self.profile = Some(updated);
        Ok(())
    }

    /// Publish the folder. A local folder warns that versioning is absent.
    pub fn publish(&mut self, store: &dyn SecretStore) -> Result<PublishReport, Error> {
        let Some(profile) = self.profile.clone() else {
            return Err(Error::Profile);
        };
        let Some(recovery_password) = store.get(Account::RecoveryPassword)? else {
            return Err(Error::MissingRecovery);
        };
        let location = self.open_location(store)?;
        let protection = self.object_protection(store)?;
        let outcome = publish_folder(
            &location,
            &recovery_password,
            &profile.device_id,
            &profile.folder,
        )?;
        Ok(PublishReport {
            outcome,
            protection,
        })
    }

    /// Read device keys that can be revoked. The recovery key is not included.
    pub fn load_device_keys(&mut self, store: &dyn SecretStore) -> Result<(), Error> {
        let Some(profile) = self.profile.clone() else {
            return Err(Error::Profile);
        };
        let Some(recovery_password) = store.get(Account::RecoveryPassword)? else {
            return Err(Error::MissingRecovery);
        };
        let location = self.open_location(store)?;
        self.device_keys = Some(list_other_device_keys(
            &location,
            &recovery_password,
            &profile.device_id,
        )?);
        Ok(())
    }

    /// Delete one device key. The key that opened the repository stays.
    pub fn revoke_device_key(
        &mut self,
        store: &dyn SecretStore,
        key_id: &str,
    ) -> Result<(), Error> {
        let Some(recovery_password) = store.get(Account::RecoveryPassword)? else {
            return Err(Error::MissingRecovery);
        };
        let location = self.open_location(store)?;
        remove_device_key(&location, &recovery_password, key_id)?;
        self.load_device_keys(store)
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
    /// The recovery password is read only to wrap that device key. It is not sent.
    pub fn approve_pending(&mut self, store: &dyn SecretStore) -> Result<(), Error> {
        if self.pending.is_none() {
            return Err(satchel_pair::Error::Closed.into());
        }
        let Some(profile) = self.profile.clone() else {
            return Err(Error::Profile);
        };
        let Some(recovery_password) = store.get(Account::RecoveryPassword)? else {
            return Err(Error::MissingRecovery);
        };
        let Some(maintenance) = store.get(Account::BucketCredential)? else {
            return Err(Error::MissingSecret);
        };
        if maintenance.is_empty() {
            return Err(Error::MissingSecret);
        }
        let Some(phone_secret) = store.get(Account::PhoneCredential)? else {
            return Err(Error::MissingPhoneCredential);
        };
        let device_id = DeviceId::parse(&self.pending.as_ref().expect("pending").device_id)?;
        let device_key_password = new_device_password();
        let location = self.open_location(store)?;
        let key_id = add_device_key(
            &location,
            &recovery_password,
            &device_id,
            &device_key_password,
        )?;
        let bucket_credential = phone_grant_secret(profile.object_store.as_ref(), &phone_secret)?;
        let secrets = DeviceSecrets {
            endpoint: grant_endpoint(&profile),
            bucket_credential,
            device_key_password,
        };
        if let Err(err) = self.finish(Decision::Approve(secrets)) {
            if let Err(remove_err) = remove_device_key(&location, &recovery_password, &key_id) {
                return Err(remove_err.into());
            }
            return Err(err);
        }
        Ok(())
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
            if let Ok(invite) = Invite::decode(&offer.invite)
                && let Ok(png) = satchel_pair::invite_qr_png(&invite)
            {
                let mut qr = item("QR".to_string(), false, TrayCommand::None);
                qr.icon_png = Some(png);
                items.push(qr);
            }
        } else if self.profile.is_some() {
            items.push(item("Sync".to_string(), true, TrayCommand::Publish));
            items.push(item(
                "Add device".to_string(),
                true,
                TrayCommand::OpenAddDevice,
            ));
            items.push(item("Devices".to_string(), true, TrayCommand::LoadDevices));
            if let Some(keys) = &self.device_keys {
                for key in keys {
                    items.push(item(
                        format!("Revoke {}", key.device_id.as_str()),
                        true,
                        TrayCommand::Revoke(key.key_id.clone()),
                    ));
                }
            }
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
        icon_png: None,
    }
}

fn grant_endpoint(profile: &Profile) -> String {
    if let Some(store) = &profile.object_store {
        return store.endpoint.clone();
    }
    profile.repository.display().to_string()
}

/// Credential placed in the grant. A directory keeps the stored text. An object
/// store adds the bucket coordinates to the phone access key.
pub fn phone_grant_secret(
    object_store: Option<&ObjectStore>,
    phone_secret: &str,
) -> Result<String, Error> {
    if phone_secret.is_empty() {
        return Err(Error::PhoneCredential);
    }
    let Some(object_store) = object_store else {
        return Ok(phone_secret.to_string());
    };
    let access: AccessKey =
        serde_json::from_str(phone_secret).map_err(|_| Error::PhoneCredential)?;
    if access.access_key_id.is_empty() || access.secret_access_key.is_empty() {
        return Err(Error::PhoneCredential);
    }
    let body = serde_json::json!({
        "access_key_id": access.access_key_id,
        "secret_access_key": access.secret_access_key,
        "bucket": object_store.bucket,
        "region": object_store.region,
        "prefix": object_store.prefix,
    });
    serde_json::to_string(&body).map_err(|_| Error::PhoneCredential)
}

#[derive(Deserialize)]
struct AccessKey {
    access_key_id: String,
    secret_access_key: String,
}

impl Desktop {
    fn open_location(&self, store: &dyn SecretStore) -> Result<Location, Error> {
        let Some(profile) = self.profile.as_ref() else {
            return Err(Error::Profile);
        };
        let Some(object_store) = profile.object_store.as_ref() else {
            return Ok(Location::Directory(profile.repository.clone()));
        };
        let Some(maintenance) = store.get(Account::BucketCredential)? else {
            return Err(Error::MissingSecret);
        };
        let access: AccessKey =
            serde_json::from_str(&maintenance).map_err(|_| Error::MissingSecret)?;
        if access.access_key_id.is_empty() || access.secret_access_key.is_empty() {
            return Err(Error::MissingSecret);
        }
        let s3 = S3Store::new(
            object_store.bucket.clone(),
            object_store.endpoint.clone(),
            object_store.region.clone(),
            object_store.prefix.clone(),
            access.access_key_id,
            access.secret_access_key,
        )?;
        Ok(Location::S3(s3))
    }

    fn object_protection(&self, store: &dyn SecretStore) -> Result<SetupCheck, Error> {
        let Some(profile) = self.profile.as_ref() else {
            return Err(Error::Profile);
        };
        let Some(object_store) = profile.object_store.as_ref() else {
            return check_setup(StorageKind::Directory, None, false).map_err(Error::from);
        };
        let Some(maintenance) = store.get(Account::BucketCredential)? else {
            return Err(Error::MissingSecret);
        };
        let access: AccessKey =
            serde_json::from_str(&maintenance).map_err(|_| Error::MissingSecret)?;
        let probed = probe_object_protection(
            &object_store.endpoint,
            &object_store.bucket,
            &object_store.region,
            &access.access_key_id,
            &access.secret_access_key,
        )?;
        check_setup(
            StorageKind::S3,
            probed.versioning,
            probed.lifecycle_deletes_objects,
        )
        .map_err(Error::from)
    }
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
