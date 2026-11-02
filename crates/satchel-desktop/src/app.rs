use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use satchel_core::{
    Client, DeviceId, DeviceKeyRecord, Location, S3Store, SetupCheck, StorageKind, StorageSetup,
    add_device_key, check_setup, create_repository, generate_recovery_password,
    list_other_device_keys, probe_object_protection, remove_device_key, sync_folder,
};
use satchel_pair::{
    ApprovalRequest, Decision, DeviceSecrets, Invite, ListenConfig, Offer, PairingSession,
};
use serde::Deserialize;

use crate::Error;
use crate::profile::{self, ObjectStore, Profile, Role};
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
    /// Open the local page that creates a repository or joins one.
    OpenSetup,
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

/// An access key pair for a bucket. The secret is never printed.
#[derive(Clone)]
pub struct AccessPair {
    /// Access key id.
    pub access_key_id: String,
    /// Secret access key.
    pub secret_access_key: String,
}

impl std::fmt::Debug for AccessPair {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("AccessPair").finish_non_exhaustive()
    }
}

/// What the person keeps after this device created the repository.
pub struct CreatedProfile {
    /// Opens the repository with the official `restic`. Shown once.
    pub recovery_password: String,
    /// Ready for object storage, a warning for a folder.
    pub protection: SetupCheck,
}

impl std::fmt::Debug for CreatedProfile {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CreatedProfile")
            .field("protection", &self.protection)
            .finish_non_exhaustive()
    }
}

/// What the invite gave the device that is joining.
#[derive(Clone, Debug)]
pub struct JoinTarget {
    /// Address of the device that holds the repository.
    pub address: SocketAddr,
    /// Six digits, with or without the display space.
    pub code: String,
    /// Certificate fingerprint from the invite.
    pub fingerprint: String,
    /// Name shown on the approval screen.
    pub device_name: String,
    /// System shown on the approval screen.
    pub os: String,
}

impl JoinTarget {
    /// Read the fields from the invite JSON the other device shows.
    pub fn from_invite(
        text: &str,
        device_name: impl Into<String>,
        os: impl Into<String>,
    ) -> Result<Self, Error> {
        let invite = Invite::decode(text)?;
        let address = invite
            .address
            .parse::<SocketAddr>()
            .map_err(|_| Error::Field { field: "address" })?;
        Ok(Self {
            address,
            code: invite.code,
            fingerprint: invite.fingerprint,
            device_name: device_name.into(),
            os: os.into(),
        })
    }
}

/// A grant and the opaque id this device sent to get it.
pub struct Granted {
    device_id: DeviceId,
    secrets: DeviceSecrets,
}

/// Pair with the device that holds the repository. Touches no desktop state.
pub fn request_grant(target: JoinTarget, timeout: Duration) -> Result<Granted, Error> {
    let device_id = DeviceId::generate();
    let secrets = satchel_pair::join(satchel_pair::JoinRequest {
        address: target.address,
        code: target.code,
        fingerprint: target.fingerprint,
        device_name: target.device_name,
        os: target.os,
        device_id: device_id.as_str().to_string(),
        claimed_address: String::new(),
        claimed_interface: String::new(),
        timeout,
    })?;
    Ok(Granted { device_id, secrets })
}

/// Placeholder stored where a folder repository has no bucket credential.
const LOCAL_FOLDER: &str = "local-folder";

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
    last_sync: Option<SyncStatus>,
}

/// How the last sync ended, for the tray. A failure keeps its reason; it never carries a secret.
#[derive(Clone, Debug, Eq, PartialEq)]
enum SyncStatus {
    Done { unix: u64, unprotected: bool },
    Failed { unix: u64, reason: String },
}

fn clock_label(unix: u64) -> String {
    let minutes = unix / 60 % (24 * 60);
    format!("{:02}:{:02} UTC", minutes / 60, minutes % 60)
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0)
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
            last_sync: None,
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

    /// Create the repository on the chosen storage and keep what this device must guard.
    ///
    /// The recovery password and the maintenance credential go to the keystore
    /// before the first write. S3, R2, and B2 need the limited device
    /// credential as well, because approving another device sends it. The
    /// setup gate runs before the bucket is written; a refusal removes what
    /// was just stored.
    pub fn create_repository(
        &mut self,
        store: &dyn SecretStore,
        folder: PathBuf,
        setup: StorageSetup,
        device_access: Option<AccessPair>,
    ) -> Result<CreatedProfile, Error> {
        if self.profile.is_some() {
            return Err(Error::ProfileExists);
        }
        let device_id = DeviceId::generate();
        let object_store = if setup.is_object_store() {
            Some(ObjectStore {
                bucket: setup.bucket.clone(),
                endpoint: setup.endpoint.clone(),
                region: setup.region.clone(),
                prefix: setup.prefix.clone(),
            })
        } else {
            None
        };
        let repository = match &object_store {
            Some(_) => self.config_dir.join("object-store"),
            None => setup.directory.clone(),
        };
        let candidate = Profile {
            folder: folder.clone(),
            repository,
            device_id: device_id.clone(),
            object_store,
            kind: setup.kind,
            role: Role::Maintainer,
        };
        profile::check_new(&self.config_dir, &candidate)?;
        let (maintenance, limited) = if setup.is_object_store() {
            let Some(device_access) = device_access else {
                return Err(Error::MissingPhoneCredential);
            };
            if device_access.access_key_id.is_empty() || device_access.secret_access_key.is_empty()
            {
                return Err(Error::PhoneCredential);
            }
            if device_access.access_key_id == setup.access_key_id {
                return Err(Error::PhoneCredential);
            }
            (
                access_json(&setup.access_key_id, &setup.secret_access_key)?,
                access_json(
                    &device_access.access_key_id,
                    &device_access.secret_access_key,
                )?,
            )
        } else {
            (LOCAL_FOLDER.to_string(), LOCAL_FOLDER.to_string())
        };
        let password = generate_recovery_password();
        store.put(Account::RecoveryPassword, &password)?;
        store.put(Account::BucketCredential, &maintenance)?;
        store.put(Account::PhoneCredential, &limited)?;
        let made = match create_repository(&setup, &device_id, &password) {
            Ok(made) => made,
            Err(err) => {
                forget_all(store);
                return Err(err.into());
            }
        };
        match profile::create_with(&self.config_dir, candidate) {
            Ok(profile) => self.profile = Some(profile),
            Err(err) => {
                forget_all(store);
                return Err(err);
            }
        }
        Ok(CreatedProfile {
            recovery_password: made.recovery_password,
            protection: made.protection,
        })
    }

    /// Join a repository another device holds. The storage account is not typed again.
    ///
    /// This blocks until the other device approves or the timeout ends. A caller
    /// that shares the `Desktop` should call [`request_grant`] outside the lock
    /// and then [`Desktop::join_with_grant`].
    pub fn join_repository(
        &mut self,
        store: &dyn SecretStore,
        folder: PathBuf,
        target: JoinTarget,
        timeout: Duration,
    ) -> Result<(), Error> {
        if self.profile.is_some() {
            return Err(Error::ProfileExists);
        }
        let granted = request_grant(target, timeout)?;
        self.join_with_grant(store, folder, granted)
    }

    /// Keep a grant: open the repository with the device key, then write the profile.
    ///
    /// The grant carries this device's own key and the limited credential. The
    /// recovery password is not part of it. The profile is written only after
    /// that key has opened the repository.
    pub fn join_with_grant(
        &mut self,
        store: &dyn SecretStore,
        folder: PathBuf,
        granted: Granted,
    ) -> Result<(), Error> {
        if self.profile.is_some() {
            return Err(Error::ProfileExists);
        }
        let Granted { device_id, secrets } = granted;
        let (candidate, limited) =
            profile_from_grant(&self.config_dir, folder, device_id, &secrets)?;
        profile::check_new(&self.config_dir, &candidate)?;
        let location = location_from(&candidate, limited.as_deref())?;
        let work = self.config_dir.join("join-check");
        let client = Client::open(
            candidate.device_id.clone(),
            &work,
            secrets.device_key_password.clone(),
        )
        .map_err(|_| Error::Grant)?;
        let opened = client.catalog(&location);
        let _ = std::fs::remove_dir_all(&work);
        opened.map_err(|_| Error::Grant)?;
        store.put(Account::DeviceKeyPassword, &secrets.device_key_password)?;
        let credential = limited.unwrap_or_else(|| LOCAL_FOLDER.to_string());
        store.put(Account::BucketCredential, &credential)?;
        match profile::create_with(&self.config_dir, candidate) {
            Ok(profile) => {
                self.profile = Some(profile);
                Ok(())
            }
            Err(err) => {
                forget_all(store);
                Err(err)
            }
        }
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

    /// Send the folder and bring in what other devices published. A local folder warns that
    /// versioning is absent. A device that joined uses its own key, not the recovery password.
    pub fn publish(&mut self, store: &dyn SecretStore) -> Result<PublishReport, Error> {
        let result = self.publish_inner(store);
        self.last_sync = Some(match &result {
            Ok(report) => SyncStatus::Done {
                unix: now_unix(),
                unprotected: report.protection == SetupCheck::Unprotected,
            },
            Err(err) => SyncStatus::Failed {
                unix: now_unix(),
                reason: err.to_string(),
            },
        });
        result
    }

    fn publish_inner(&self, store: &dyn SecretStore) -> Result<PublishReport, Error> {
        let Some(profile) = self.profile.clone() else {
            return Err(Error::Profile);
        };
        let account = match profile.role {
            Role::Maintainer => Account::RecoveryPassword,
            Role::Member => Account::DeviceKeyPassword,
        };
        let Some(password) = store.get(account)? else {
            return Err(match profile.role {
                Role::Maintainer => Error::MissingRecovery,
                Role::Member => Error::MissingSecret,
            });
        };
        let location = self.open_location(store)?;
        let protection = self.object_protection(store)?;
        let outcome = sync_folder(&location, &password, &profile.device_id, &profile.folder)?;
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
        let device_key_password = generate_recovery_password();
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
            items.push(item(
                format!("Storage {}", storage_label(profile)),
                false,
                TrayCommand::None,
            ));
            items.push(item(
                match profile.role {
                    Role::Maintainer => "Role maintainer".to_string(),
                    Role::Member => "Role device".to_string(),
                },
                false,
                TrayCommand::None,
            ));
            match &self.last_sync {
                None => items.push(item("Not synced yet".to_string(), false, TrayCommand::None)),
                Some(SyncStatus::Done { unix, unprotected }) => {
                    items.push(item(
                        format!("Synced {}", clock_label(*unix)),
                        false,
                        TrayCommand::None,
                    ));
                    if *unprotected {
                        items.push(item(
                            "No object versioning here: an overwritten file cannot come back"
                                .to_string(),
                            false,
                            TrayCommand::None,
                        ));
                    }
                }
                Some(SyncStatus::Failed { unix, reason }) => {
                    items.push(item(
                        format!("Sync failed {}", clock_label(*unix)),
                        false,
                        TrayCommand::None,
                    ));
                    items.push(item(reason.clone(), false, TrayCommand::None));
                }
            }
        } else {
            items.push(item("No folder".to_string(), false, TrayCommand::None));
            items.push(item(
                "Create or join…".to_string(),
                true,
                TrayCommand::OpenSetup,
            ));
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
        } else if let Some(profile) = &self.profile {
            items.push(item("Sync".to_string(), true, TrayCommand::Publish));
            if profile.role == Role::Maintainer {
                items.push(item(
                    "Add device".to_string(),
                    true,
                    TrayCommand::OpenAddDevice,
                ));
                items.push(item("Devices".to_string(), true, TrayCommand::LoadDevices));
            }
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

/// Template pixels for the macOS status item: 36×36, black RGB, mark in the alpha.
pub fn tray_mark_rgba() -> &'static [u8] {
    include_bytes!("../../../assets/satchel-tray.rgba")
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

fn storage_label(profile: &Profile) -> String {
    let name = match profile.kind {
        StorageKind::Directory => "folder",
        StorageKind::S3 => "S3",
        StorageKind::R2 => "R2",
        StorageKind::B2 => "B2",
        StorageKind::WebDav => "WebDAV",
        StorageKind::Sftp => "SFTP",
    };
    match &profile.object_store {
        Some(store) if store.prefix.is_empty() => format!("{name} {}", store.bucket),
        Some(store) => format!("{name} {}/{}", store.bucket, store.prefix),
        None => format!("{name} {}", profile.repository.display()),
    }
}

fn access_json(access_key_id: &str, secret_access_key: &str) -> Result<String, Error> {
    serde_json::to_string(&serde_json::json!({
        "access_key_id": access_key_id,
        "secret_access_key": secret_access_key,
    }))
    .map_err(|_| Error::PhoneCredential)
}

fn forget_all(store: &dyn SecretStore) {
    let _ = store.delete(Account::RecoveryPassword);
    let _ = store.delete(Account::BucketCredential);
    let _ = store.delete(Account::PhoneCredential);
    let _ = store.delete(Account::DeviceKeyPassword);
}

#[derive(Deserialize)]
struct GrantAccess {
    access_key_id: String,
    secret_access_key: String,
    #[serde(default)]
    bucket: String,
    #[serde(default)]
    region: String,
    #[serde(default)]
    prefix: String,
}

/// Profile for a device that joined, and the access key JSON to store for it.
fn profile_from_grant(
    config_dir: &std::path::Path,
    folder: PathBuf,
    device_id: DeviceId,
    secrets: &DeviceSecrets,
) -> Result<(Profile, Option<String>), Error> {
    if secrets.endpoint.starts_with("http://") || secrets.endpoint.starts_with("https://") {
        let access: GrantAccess =
            serde_json::from_str(&secrets.bucket_credential).map_err(|_| Error::Grant)?;
        let limited = access_json(&access.access_key_id, &access.secret_access_key)?;
        let profile = Profile {
            folder,
            repository: config_dir.join("object-store"),
            device_id,
            object_store: Some(ObjectStore {
                bucket: access.bucket,
                endpoint: secrets.endpoint.clone(),
                region: access.region,
                prefix: access.prefix,
            }),
            kind: StorageKind::S3,
            role: Role::Member,
        };
        return Ok((profile, Some(limited)));
    }
    if secrets.endpoint.is_empty() || secrets.device_key_password.is_empty() {
        return Err(Error::Grant);
    }
    let profile = Profile {
        folder,
        repository: PathBuf::from(&secrets.endpoint),
        device_id,
        object_store: None,
        kind: StorageKind::Directory,
        role: Role::Member,
    };
    Ok((profile, None))
}

fn location_from(profile: &Profile, limited: Option<&str>) -> Result<Location, Error> {
    let Some(object_store) = &profile.object_store else {
        return Ok(Location::Directory(profile.repository.clone()));
    };
    let Some(limited) = limited else {
        return Err(Error::Grant);
    };
    let access: AccessKey = serde_json::from_str(limited).map_err(|_| Error::Grant)?;
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
