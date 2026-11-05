use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use jiff::{Timestamp, tz::TimeZone};
use satchel_core::{
    Client, DeviceId, DeviceKeyRecord, Location, QuietMark, S3Store, SetupCheck, StorageKind,
    StorageSetup, WakeEndpoint, WakeListener, change_recovery_password, check_setup,
    create_repository, generate_recovery_password, list_other_device_keys, probe_object_protection,
    provision_device_key, quiet_mark, remove_device_key, send_wakes, sync_folder_announcing,
};
use satchel_pair::{
    ApprovalRequest, Decision, DeviceSecrets, Invite, ListenConfig, Offer, PairingSession,
};
use serde::Deserialize;

use crate::Error;
use crate::profile::{self, ObjectStore, Profile, Role};
use crate::secrets::{Account, SecretStore};
use crate::text::Language;

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
    /// Wake notices sent because this sync wrote a snapshot.
    pub wakes_sent: usize,
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
    /// State after the last full sync, so the timer can skip a cycle with nothing to do.
    quiet: Option<(QuietMark, SetupCheck)>,
    /// Where this desktop listens for wake notices, while it does.
    wake: Option<WakeEndpoint>,
    /// The language of the tray, from the system unless set.
    language: Language,
}

/// How the last sync ended, for the tray. The error itself goes to the log, not the menu.
#[derive(Clone, Debug, Eq, PartialEq)]
enum SyncStatus {
    Done { unix: u64, unprotected: bool },
    Failed { unix: u64, hint: FailureHint },
}

/// What a failed sync asks the person to do next.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FailureHint {
    /// The keystore is locked or lost an entry.
    Keystore,
    /// The storage did not answer or refused the request.
    Storage,
    /// The synced folder is missing or unreadable.
    Folder,
    /// Anything else. The log has the details.
    Other,
}

fn failure_hint(err: &Error) -> FailureHint {
    use satchel_core::Error as Core;
    match err {
        Error::Store { .. }
        | Error::MissingSecret
        | Error::MissingRecovery
        | Error::MissingPhoneCredential => FailureHint::Keystore,
        Error::Path { .. }
        | Error::Io
        | Error::Device(Core::FolderUnavailable | Core::SourceDirectory { .. }) => {
            FailureHint::Folder
        }
        Error::Grant
        | Error::Device(
            Core::Repository { .. } | Core::Probe | Core::ProbeDenied | Core::ObjectProtection,
        ) => FailureHint::Storage,
        _ => FailureHint::Other,
    }
}

/// `14:32` on the same local day as `now`, `2026-11-04 14:32` before it.
fn clock_label(unix: u64, now: u64, zone: &TimeZone) -> String {
    let at = |seconds: u64| {
        Timestamp::from_second(i64::try_from(seconds).unwrap_or(0))
            .unwrap_or(Timestamp::UNIX_EPOCH)
            .to_zoned(zone.clone())
    };
    let (then, today) = (at(unix), at(now));
    if then.date() == today.date() {
        then.strftime("%H:%M").to_string()
    } else {
        then.strftime("%Y-%m-%d %H:%M").to_string()
    }
}

fn local_clock(unix: u64) -> String {
    clock_label(unix, now_unix(), &TimeZone::system())
}

/// The folder with the home directory written as `~`.
fn folder_label(folder: &std::path::Path) -> String {
    if let Some(home) = std::env::var_os("HOME")
        && let Ok(rest) = folder.strip_prefix(&home)
    {
        if rest.as_os_str().is_empty() {
            return "~".to_string();
        }
        return format!("~/{}", rest.display());
    }
    folder.display().to_string()
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
            quiet: None,
            wake: None,
            language: Language::detect(),
        })
    }

    /// The language of the tray and the setup page's fallback.
    pub fn language(&self) -> Language {
        self.language
    }

    /// Use `language` instead of the system's.
    pub fn set_language(&mut self, language: Language) {
        self.language = language;
    }

    /// The same desktop, speaking `language`.
    pub fn with_language(mut self, language: Language) -> Self {
        self.language = language;
        self
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
        self.quiet = None;
        let mut result = self.publish_inner(store);
        if let Ok(report) = &mut result {
            if report.outcome.wrote
                && let Some(profile) = &self.profile
            {
                report.wakes_sent =
                    send_wakes(&profile.device_id, &report.outcome.peers, now_unix());
            }
            self.remember_quiet(store, report.protection);
        }
        self.last_sync = Some(match &result {
            Ok(report) => SyncStatus::Done {
                unix: now_unix(),
                unprotected: report.protection == SetupCheck::Unprotected,
            },
            Err(err) => SyncStatus::Failed {
                unix: now_unix(),
                hint: failure_hint(err),
            },
        });
        result
    }

    /// Listen for wake notices on `ip` and announce it on the next sync.
    ///
    /// The caller owns the listener and waits on it outside the lock. `ip` should
    /// be this machine's address on the mesh network: peers elsewhere cannot reach
    /// it, and a LAN address would take notices from anyone on the LAN.
    pub fn listen_for_wakes(&mut self, ip: std::net::IpAddr) -> Result<WakeListener, Error> {
        let (listener, endpoint) =
            satchel_core::bind_saved_wake(&self.config_dir.join("wake.json"), ip)?;
        if self.wake.as_ref() != Some(&endpoint) {
            self.quiet = None;
        }
        self.wake = Some(endpoint);
        Ok(listener)
    }

    /// The timer's sync. When neither the folder nor the repository's snapshot
    /// list changed since the last full sync, this costs one listing and returns
    /// `None` instead of opening the repository.
    pub fn auto_sync(&mut self, store: &dyn SecretStore) -> Result<Option<PublishReport>, Error> {
        if let (Some((previous, protection)), Some(profile)) = (&self.quiet, &self.profile) {
            let protection = *protection;
            let location = self.open_location(store)?;
            if quiet_mark(&location, &profile.folder).ok().as_ref() == Some(previous) {
                self.last_sync = Some(SyncStatus::Done {
                    unix: now_unix(),
                    unprotected: protection == SetupCheck::Unprotected,
                });
                return Ok(None);
            }
        }
        self.publish(store).map(Some)
    }

    fn remember_quiet(&mut self, store: &dyn SecretStore, protection: SetupCheck) {
        let Some(profile) = &self.profile else {
            return;
        };
        let Ok(location) = self.open_location(store) else {
            return;
        };
        self.quiet = quiet_mark(&location, &profile.folder)
            .ok()
            .map(|mark| (mark, protection));
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
        let outcome = sync_folder_announcing(
            &location,
            &password,
            &profile.device_id,
            &profile.folder,
            self.wake.clone(),
        )?;
        Ok(PublishReport {
            outcome,
            protection,
            wakes_sent: 0,
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

    /// Replace the recovery password. Only the device that created the repository can.
    ///
    /// The new value is kept in the keystore before the repository changes, so a
    /// crash between the two steps never loses the only copy. Device keys keep
    /// working: the master key does not change. Returns the new password for the
    /// caller to show once.
    pub fn change_recovery_password(&mut self, store: &dyn SecretStore) -> Result<String, Error> {
        let Some(profile) = self.profile.as_ref() else {
            return Err(Error::Profile);
        };
        if profile.role != Role::Maintainer {
            return Err(Error::NotMaintainer);
        }
        let Some(old) = store.get(Account::RecoveryPassword)? else {
            return Err(Error::MissingRecovery);
        };
        let location = self.open_location(store)?;
        let new = generate_recovery_password();
        store.put(Account::PendingRecoveryPassword, &new)?;
        if let Err(err) = change_recovery_password(&location, &old, &new) {
            let _ = store.delete(Account::PendingRecoveryPassword);
            return Err(err.into());
        }
        store.put(Account::RecoveryPassword, &new)?;
        let _ = store.delete(Account::PendingRecoveryPassword);
        Ok(new)
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
        let location = self.open_location(store)?;
        let provisioned = provision_device_key(&location, &recovery_password, &device_id)?;
        let key_id = provisioned.key_id;
        let device_key_password = provisioned.device_key_password;
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
        let t = |en: &'static str, pt: &'static str| self.language.pick(en, pt);
        let info = |label: String| item(label, false, TrayCommand::None);
        let mut items = Vec::new();
        if let Some(profile) = &self.profile {
            items.push(info(format!(
                "{} {}",
                t("Folder:", "Pasta:"),
                folder_label(&profile.folder)
            )));
            items.push(info(format!(
                "{} {}",
                t("Storage:", "Armazenamento:"),
                storage_label(self.language, profile)
            )));
            items.push(info(
                match profile.role {
                    Role::Maintainer => t(
                        "Main device: adds and removes devices",
                        "Aparelho principal: adiciona e remove aparelhos",
                    ),
                    Role::Member => t(
                        "Added through another device",
                        "Adicionado por outro aparelho",
                    ),
                }
                .to_string(),
            ));
            match &self.last_sync {
                None => items.push(info(
                    t("Not synced yet", "Ainda não sincronizado").to_string(),
                )),
                Some(SyncStatus::Done { unix, unprotected }) => {
                    items.push(info(format!(
                        "{} {}",
                        t("Synced at", "Sincronizado às"),
                        local_clock(*unix)
                    )));
                    if *unprotected {
                        items.push(info(
                            t(
                                "This storage keeps no old versions: an overwritten file cannot come back",
                                "Este armazenamento não guarda versões antigas: um arquivo sobrescrito não volta",
                            )
                            .to_string(),
                        ));
                    }
                }
                Some(SyncStatus::Failed { unix, hint }) => {
                    items.push(info(format!(
                        "{} {}",
                        t("Sync failed at", "A sincronização falhou às"),
                        local_clock(*unix)
                    )));
                    items.push(info(
                        match hint {
                            FailureHint::Keystore => t(
                                "Unlock the keychain, then sync again",
                                "Desbloqueie o chaveiro e sincronize de novo",
                            ),
                            FailureHint::Storage => t(
                                "The storage did not answer. Check the network, then sync again",
                                "O armazenamento não respondeu. Confira a rede e sincronize de novo",
                            ),
                            FailureHint::Folder => t(
                                "The folder is missing or unreadable. Check it, then sync again",
                                "A pasta sumiu ou não pode ser lida. Confira e sincronize de novo",
                            ),
                            FailureHint::Other => t(
                                "Sync again. If it keeps failing, see the log",
                                "Sincronize de novo. Se continuar falhando, veja o log",
                            ),
                        }
                        .to_string(),
                    ));
                }
            }
        } else {
            items.push(info(
                t("No folder set up", "Nenhuma pasta configurada").to_string(),
            ));
            items.push(item(
                t("Create or join…", "Criar ou entrar…").to_string(),
                true,
                TrayCommand::OpenSetup,
            ));
        }
        let mut invite = None;
        if let Some(pending) = &self.pending {
            items.push(info(match self.language {
                Language::En => format!("“{}” wants to join", pending.device_name),
                Language::PtBr => format!("“{}” quer entrar", pending.device_name),
            }));
            items.push(info(format!("{} {}", t("System:", "Sistema:"), pending.os)));
            items.push(info(match self.language {
                Language::En => format!(
                    "From {} on {}",
                    pending.peer_address.ip(),
                    pending.local_interface
                ),
                Language::PtBr => format!(
                    "De {} pela {}",
                    pending.peer_address.ip(),
                    pending.local_interface
                ),
            }));
            items.push(info(format!(
                "{} {}",
                t("Code typed:", "Código digitado:"),
                pending.code
            )));
            items.push(item(
                format!("{} “{}”", t("Approve", "Aprovar"), pending.device_name),
                true,
                TrayCommand::Approve,
            ));
            items.push(item(
                t("Deny", "Recusar").to_string(),
                true,
                TrayCommand::Deny,
            ));
        } else if let Some(session) = &self.session {
            let offer = session.offer();
            invite = Some(offer.invite.clone());
            items.push(info(format!(
                "{} {}",
                t("Type on the other device:", "Digite no outro aparelho:"),
                offer.code
            )));
            if let Ok(invite) = Invite::decode(&offer.invite)
                && let Ok(png) = satchel_pair::invite_qr_png(&invite)
            {
                let mut qr = info(
                    t(
                        "Or scan this code with the phone",
                        "Ou leia este código com o celular",
                    )
                    .to_string(),
                );
                qr.icon_png = Some(png);
                items.push(qr);
            }
            items.push(info(format!(
                "{} {}",
                t("Address:", "Endereço:"),
                offer.address
            )));
            items.push(info(format!(
                "{} {}",
                t("Fingerprint:", "Impressão digital:"),
                grouped(&offer.fingerprint, 4)
            )));
            items.push(item(
                t("Stop adding", "Parar de adicionar").to_string(),
                true,
                TrayCommand::CloseAddDevice,
            ));
        } else if let Some(profile) = &self.profile {
            items.push(item(
                t("Sync now", "Sincronizar agora").to_string(),
                true,
                TrayCommand::Publish,
            ));
            if profile.role == Role::Maintainer {
                items.push(item(
                    t("Add device…", "Adicionar aparelho…").to_string(),
                    true,
                    TrayCommand::OpenAddDevice,
                ));
                items.push(item(
                    t("Show devices", "Mostrar aparelhos").to_string(),
                    true,
                    TrayCommand::LoadDevices,
                ));
                items.push(item(
                    t("Recovery password…", "Senha de recuperação…").to_string(),
                    true,
                    TrayCommand::OpenSetup,
                ));
            }
            if let Some(keys) = &self.device_keys {
                if keys.is_empty() {
                    items.push(info(
                        t("No other device yet", "Nenhum outro aparelho ainda").to_string(),
                    ));
                }
                for key in keys {
                    items.push(item(
                        format!(
                            "{} {}",
                            t("Remove device", "Remover aparelho"),
                            grouped(key.device_id.as_str(), 4)
                        ),
                        true,
                        TrayCommand::Revoke(key.key_id.clone()),
                    ));
                }
            }
        }
        items.push(item(
            t("Quit Satchel", "Sair do Satchel").to_string(),
            true,
            TrayCommand::Quit,
        ));
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

/// `text` in groups of `size` characters, separated by spaces, for reading aloud or comparing.
fn grouped(text: &str, size: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    chars
        .chunks(size)
        .map(|chunk| chunk.iter().collect::<String>())
        .collect::<Vec<_>>()
        .join(" ")
}

fn storage_label(language: Language, profile: &Profile) -> String {
    let name = match profile.kind {
        StorageKind::Directory => language.pick("Folder", "Pasta"),
        StorageKind::S3 => "S3",
        StorageKind::R2 => "Cloudflare R2",
        StorageKind::B2 => "Backblaze B2",
        StorageKind::WebDav => "WebDAV",
        StorageKind::Sftp => "SFTP",
    };
    match &profile.object_store {
        Some(store) if store.prefix.is_empty() => format!("{name} · {}", store.bucket),
        Some(store) => format!("{name} · {}/{}", store.bucket, store.prefix),
        None => format!("{name} · {}", folder_label(&profile.repository)),
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
    let _ = store.delete(Account::PendingRecoveryPassword);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_clock_is_local_and_shows_the_date_only_for_an_earlier_day() {
        let sao_paulo = TimeZone::fixed(jiff::tz::offset(-3));
        // 2023-11-14 22:13:20 UTC is 19:13 in São Paulo, the same day.
        let synced = 1_700_000_000;
        assert_eq!(clock_label(synced, synced + 60, &sao_paulo), "19:13");
        assert_eq!(clock_label(synced, synced + 60, &TimeZone::UTC), "22:13");
        // Two hours later it is past midnight in UTC and still the 14th in São Paulo.
        let later = synced + 2 * 3600;
        assert_eq!(clock_label(synced, later, &sao_paulo), "19:13");
        assert_eq!(
            clock_label(synced, later, &TimeZone::UTC),
            "2023-11-14 22:13"
        );
    }

    #[test]
    fn a_failed_sync_names_what_to_check_without_the_error_text() {
        assert_eq!(
            failure_hint(&Error::Store { kind: "locked" }),
            FailureHint::Keystore
        );
        assert_eq!(failure_hint(&Error::MissingSecret), FailureHint::Keystore);
        assert_eq!(
            failure_hint(&Error::Device(satchel_core::Error::Repository {
                message: "connection refused".to_string()
            })),
            FailureHint::Storage
        );
        assert_eq!(
            failure_hint(&Error::Device(satchel_core::Error::FolderUnavailable)),
            FailureHint::Folder
        );
        assert_eq!(failure_hint(&Error::Profile), FailureHint::Other);
    }

    #[test]
    fn ids_are_grouped_for_reading() {
        assert_eq!(grouped("0123456789ab", 4), "0123 4567 89ab");
        assert_eq!(grouped("01234", 4), "0123 4");
        assert_eq!(grouped("", 4), "");
    }
}
