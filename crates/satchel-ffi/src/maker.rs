//! A phone that creates the repository, and a phone that shows the invite.
//!
//! The creator keeps the recovery password and the maintenance credential in
//! its grant file. Another device never receives them: a joining device gets
//! its own key and the limited credential. Which device creates is decided by
//! the person, not by the operating system.

use std::net::SocketAddr;
use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use satchel_core::{
    DeviceId, Location, StorageKind, StorageSetup, create_repository, generate_recovery_password,
    provision_device_key, remove_device_key,
};
use satchel_pair::{
    ApprovalRequest, Decision, DeviceSecrets, Invite, ListenConfig, PairingSession,
};
use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

use crate::{CatalogFailure, GrantSpec, open_grant, read_grant};

#[cfg(test)]
#[path = "../../satchel-core/tests/support/floci.rs"]
mod floci_support;

const LOCAL_FOLDER: &str = "local-folder";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateSpec {
    kind: String,
    #[serde(default)]
    directory: String,
    #[serde(default)]
    endpoint: String,
    #[serde(default)]
    bucket: String,
    #[serde(default)]
    region: String,
    #[serde(default)]
    prefix: String,
    #[serde(default)]
    access_key_id: String,
    #[serde(default)]
    secret_access_key: String,
    #[serde(default)]
    device_access_key_id: String,
    #[serde(default)]
    device_secret_access_key: String,
    /// Opaque id this phone already uses. Generated when absent.
    #[serde(default)]
    device_id: String,
}

/// The typed secrets do not outlive the call that used them.
impl Drop for CreateSpec {
    fn drop(&mut self) {
        self.secret_access_key.zeroize();
        self.device_secret_access_key.zeroize();
        self.access_key_id.zeroize();
        self.device_access_key_id.zeroize();
    }
}

#[derive(Serialize)]
struct CreatorGrant<'a> {
    endpoint: &'a str,
    bucket_credential: &'a str,
    device_key_password: &'a str,
    device_id: &'a str,
    work: &'a str,
    role: &'a str,
    limited_credential: &'a str,
}

fn access_doc(setup: &StorageSetup, key: &str, secret: &str) -> Result<String, CatalogFailure> {
    serde_json::to_string(&serde_json::json!({
        "access_key_id": key,
        "secret_access_key": secret,
        "bucket": setup.bucket,
        "region": setup.region,
        "prefix": setup.prefix,
    }))
    .map_err(|_| CatalogFailure)
}

static LAST_FAILURE: Mutex<&'static str> = Mutex::new("");

fn fail(reason: &'static str) -> CatalogFailure {
    *LAST_FAILURE.lock().unwrap_or_else(|p| p.into_inner()) = reason;
    CatalogFailure
}

/// Why the last `create_phone_repository` was refused. Never carries a field value.
pub(crate) fn last_failure() -> String {
    LAST_FAILURE
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .to_string()
}

/// Create the repository and return `recovery password\ngrant JSON`.
///
/// The caller keeps the grant in the system keystore; this function writes nothing
/// but the repository. S3, R2, and B2 are probed first; versioning off or a lifecycle
/// rule that deletes objects refuses the setup before anything is written.
pub(crate) fn create_phone_repository(
    spec_json: &str,
    work: &Path,
) -> Result<String, CatalogFailure> {
    *LAST_FAILURE.lock().unwrap_or_else(|p| p.into_inner()) = "";
    let spec: CreateSpec = serde_json::from_str(spec_json).map_err(|_| fail("fields"))?;
    let kind = match spec.kind.as_str() {
        "folder" => StorageKind::Directory,
        "s3" => StorageKind::S3,
        "r2" => StorageKind::R2,
        "b2" => StorageKind::B2,
        _ => return Err(fail("fields")),
    };
    let mut setup = StorageSetup::directory(&spec.directory);
    setup.kind = kind;
    let (endpoint, maintenance, limited);
    if setup.is_object_store() {
        setup.endpoint = spec.endpoint.clone();
        setup.bucket = spec.bucket.clone();
        setup.region = spec.region.clone();
        setup.prefix = spec.prefix.clone();
        setup.access_key_id = spec.access_key_id.clone();
        setup.secret_access_key = spec.secret_access_key.clone();
        if spec.device_access_key_id.is_empty()
            || spec.device_secret_access_key.is_empty()
            || spec.device_access_key_id == spec.access_key_id
        {
            return Err(fail("fields"));
        }
        setup.location().map_err(|_| fail("fields"))?;
        endpoint = spec.endpoint.clone();
        maintenance = access_doc(&setup, &spec.access_key_id, &spec.secret_access_key)?;
        limited = access_doc(
            &setup,
            &spec.device_access_key_id,
            &spec.device_secret_access_key,
        )?;
    } else {
        if spec.directory.is_empty() {
            return Err(fail("fields"));
        }
        endpoint = spec.directory.clone();
        maintenance = LOCAL_FOLDER.to_string();
        limited = LOCAL_FOLDER.to_string();
    }
    let device_id = if spec.device_id.is_empty() {
        DeviceId::generate()
    } else {
        DeviceId::parse(&spec.device_id).map_err(|_| fail("fields"))?
    };
    let password = generate_recovery_password();
    let work_text = work.to_str().ok_or_else(|| fail("fields"))?;
    let body = serde_json::to_string(&CreatorGrant {
        endpoint: &endpoint,
        bucket_credential: &maintenance,
        device_key_password: &password,
        device_id: device_id.as_str(),
        work: work_text,
        role: "maintainer",
        limited_credential: &limited,
    })
    .map_err(|_| fail("fields"))?;
    if let Err(err) = create_repository(&setup, &device_id, &password) {
        return Err(fail(match err {
            satchel_core::Error::ObjectProtection => "protection",
            satchel_core::Error::ProbeDenied => "denied",
            satchel_core::Error::RepositoryExists => "exists",
            satchel_core::Error::EmptyField { .. } => "fields",
            _ => "unreachable",
        }));
    }
    Ok(format!("{password}\n{body}"))
}

/// True when the grant file belongs to the device that created the repository.
pub(crate) fn is_maintainer(grant_path: &Path) -> bool {
    match read_grant(grant_path) {
        Ok(spec) => spec.role == "maintainer",
        Err(_) => false,
    }
}

struct Holding {
    session: PairingSession,
    pending: Option<ApprovalRequest>,
}

static HOLDING: Mutex<Option<Holding>> = Mutex::new(None);

fn holding() -> MutexGuard<'static, Option<Holding>> {
    match HOLDING.lock() {
        Ok(guard) => guard,
        Err(poison) => poison.into_inner(),
    }
}

#[derive(Serialize)]
struct OfferJson<'a> {
    code: &'a str,
    address: String,
    fingerprint: &'a str,
    invite: &'a str,
}

/// Start showing the invite. `bind` is `ip:port`, or empty to pick the mesh or LAN address.
pub(crate) fn hold_start(
    grant_path: &Path,
    bind: &str,
    timeout: std::time::Duration,
) -> Result<String, CatalogFailure> {
    if !is_maintainer(grant_path) {
        return Err(CatalogFailure);
    }
    let bind: SocketAddr = if bind.is_empty() {
        let ip = satchel_pair::preferred_bind_ip().ok_or(CatalogFailure)?;
        SocketAddr::new(ip, 0)
    } else {
        bind.parse().map_err(|_| CatalogFailure)?
    };
    let mut guard = holding();
    if guard.is_some() {
        return Err(CatalogFailure);
    }
    let session = PairingSession::listen(ListenConfig {
        bind,
        timeout,
        advertise: false,
    })
    .map_err(|_| CatalogFailure)?;
    let offer = session.offer();
    let json = serde_json::to_string(&OfferJson {
        code: &offer.code,
        address: offer.address.to_string(),
        fingerprint: &offer.fingerprint,
        invite: &offer.invite,
    })
    .map_err(|_| CatalogFailure)?;
    *guard = Some(Holding {
        session,
        pending: None,
    });
    Ok(json)
}

#[derive(Serialize)]
struct ApprovalJson<'a> {
    #[serde(rename = "deviceName")]
    device_name: &'a str,
    os: &'a str,
    #[serde(rename = "peerAddress")]
    peer_address: String,
    code: &'a str,
}

/// The approval to show, once the other device passed the code check. Empty until then.
pub(crate) fn hold_poll() -> Result<String, CatalogFailure> {
    let mut guard = holding();
    let Some(held) = guard.as_mut() else {
        return Err(CatalogFailure);
    };
    if held.pending.is_none()
        && let Ok(Some(request)) = held.session.try_approval()
    {
        held.pending = Some(request);
    }
    let Some(pending) = held.pending.as_ref() else {
        return Ok(String::new());
    };
    serde_json::to_string(&ApprovalJson {
        device_name: &pending.device_name,
        os: &pending.os,
        peer_address: pending.peer_address.to_string(),
        code: &pending.code,
    })
    .map_err(|_| CatalogFailure)
}

/// Approve or deny the approval on screen, then stop listening.
///
/// Approving writes a key for the other device's opaque id, wrapped by a new
/// password, and sends that password and the limited credential. The recovery
/// password and the maintenance credential stay in `grant_path`.
pub(crate) fn hold_decide(grant_path: &Path, approve: bool) -> Result<(), CatalogFailure> {
    let mut guard = holding();
    let Some(held) = guard.take() else {
        return Err(CatalogFailure);
    };
    let Some(pending) = held.pending else {
        return Err(CatalogFailure);
    };
    if !approve {
        return held
            .session
            .decide(Decision::Deny)
            .map_err(|_| CatalogFailure);
    }
    let spec = read_grant(grant_path)?;
    if spec.role != "maintainer" {
        let _ = held.session.decide(Decision::Deny);
        return Err(CatalogFailure);
    }
    let result = approve_with(&spec, &pending, &held.session);
    if result.is_err() {
        let _ = held.session.decide(Decision::Deny);
    }
    result
}

fn approve_with(
    spec: &GrantSpec,
    pending: &ApprovalRequest,
    session: &PairingSession,
) -> Result<(), CatalogFailure> {
    let location: Location = open_grant(spec)?;
    let other = DeviceId::parse(&pending.device_id).map_err(|_| CatalogFailure)?;
    let provisioned = provision_device_key(&location, &spec.device_key_password, &other)
        .map_err(|_| CatalogFailure)?;
    let secrets = DeviceSecrets {
        endpoint: spec.endpoint.clone(),
        bucket_credential: spec.limited_credential.clone(),
        device_key_password: provisioned.device_key_password,
    };
    if session.decide(Decision::Approve(secrets)).is_err() {
        let _ = remove_device_key(&location, &spec.device_key_password, &provisioned.key_id);
        return Err(CatalogFailure);
    }
    Ok(())
}

/// Stop listening without a decision.
pub(crate) fn hold_close() {
    holding().take();
}

/// PNG of the QR for an invite the phone shows.
pub(crate) fn invite_png(invite: &str) -> Result<Vec<u8>, CatalogFailure> {
    let invite = Invite::decode(invite).map_err(|_| CatalogFailure)?;
    satchel_pair::invite_qr_png(&invite).map_err(|_| CatalogFailure)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::sync::Mutex;
    use std::thread;
    use std::time::{Duration, Instant};

    use super::*;
    use crate::{assemble_grant_json, catalog_grant, join_pairing};

    static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

    struct Temp(PathBuf);

    impl Temp {
        fn new(tag: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "satchel-maker-{tag}-{}-{}",
                std::process::id(),
                DeviceId::generate().as_str()
            ));
            fs::create_dir_all(&path).expect("temp");
            Self(path)
        }
    }

    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn folder_spec(root: &Path) -> String {
        serde_json::json!({
            "kind": "folder",
            "directory": root.join("repository").to_str().expect("path"),
        })
        .to_string()
    }

    /// `(recovery password, grant JSON)` from a fresh folder repository.
    fn made(root: &Path) -> (String, String) {
        let created =
            create_phone_repository(&folder_spec(root), &root.join("work")).expect("create");
        let (password, grant) = created.split_once('\n').expect("two parts");
        (password.to_string(), grant.to_string())
    }

    fn key_files(root: &Path) -> Vec<String> {
        let mut texts = Vec::new();
        for entry in fs::read_dir(root.join("repository/keys")).expect("keys") {
            texts.push(fs::read_to_string(entry.expect("entry").path()).expect("key"));
        }
        texts
    }

    #[test]
    fn phone_creates_a_folder_repository_and_keeps_the_password_out_of_the_invite() {
        let _guard = ONE_AT_A_TIME.lock().unwrap_or_else(|p| p.into_inner());
        let root = Temp::new("hold");
        let (password, grant_text) = made(&root.0);
        let grant = Path::new(&grant_text);
        assert_eq!(password.len(), 64);
        assert!(
            !root.0.join("grant.json").exists(),
            "nothing is written to disk"
        );
        assert!(is_maintainer(grant));
        assert_eq!(catalog_grant(grant).expect("catalog"), "[]");
        assert!(create_phone_repository(&folder_spec(&root.0), &root.0.join("w")).is_err());
        assert_eq!(last_failure(), "exists");

        let offer = hold_start(grant, "127.0.0.1:0", Duration::from_secs(60)).expect("start");
        let offer: serde_json::Value = serde_json::from_str(&offer).expect("offer json");
        assert!(!offer.to_string().contains(&password));
        let address = offer["address"].as_str().expect("address").to_string();
        let code = offer["code"].as_str().expect("code").to_string();
        let fingerprint = offer["fingerprint"].as_str().expect("fp").to_string();
        let png = invite_png(offer["invite"].as_str().expect("invite")).expect("png");
        assert!(png.starts_with(b"\x89PNG"));

        let other = DeviceId::generate();
        let other_text = other.as_str().to_string();
        let joiner = thread::spawn(move || {
            join_pairing(
                &address,
                &code,
                &fingerprint,
                "Tablet",
                "android",
                &other_text,
                Duration::from_secs(60),
            )
        });
        let deadline = Instant::now() + Duration::from_secs(60);
        let mut shown = String::new();
        while shown.is_empty() && Instant::now() < deadline {
            shown = hold_poll().expect("poll");
            thread::sleep(Duration::from_millis(20));
        }
        assert!(shown.contains("Tablet"));
        assert!(!shown.contains(&password));
        hold_decide(grant, true).expect("approve");
        let joined = joiner.join().expect("thread").expect("joined");
        assert!(!joined.contains(&password));
        assert!(!joined.contains("recovery"));
        let member = root.0.join("member-grant.json");
        let stored = assemble_grant_json(
            &joined,
            other.as_str(),
            root.0.join("member-work").to_str().expect("work"),
        )
        .expect("assemble");
        fs::write(&member, stored).expect("write member");
        assert_eq!(catalog_grant(&member).expect("member opens"), "[]");
        assert!(!is_maintainer(&member));
        assert!(hold_start(&member, "127.0.0.1:0", Duration::from_secs(5)).is_err());

        let keys = key_files(&root.0);
        assert_eq!(keys.len(), 2);
        for text in &keys {
            assert!(!text.contains("created"));
        }
        // The session ended with the decision.
        assert!(hold_poll().is_err());
    }

    #[test]
    fn denied_invite_writes_no_key() {
        let _guard = ONE_AT_A_TIME.lock().unwrap_or_else(|p| p.into_inner());
        let root = Temp::new("deny");
        let (_, grant_text) = made(&root.0);
        let grant = Path::new(&grant_text);
        let offer = hold_start(grant, "127.0.0.1:0", Duration::from_secs(60)).expect("start");
        let offer: serde_json::Value = serde_json::from_str(&offer).expect("offer json");
        let address = offer["address"].as_str().expect("address").to_string();
        let code = offer["code"].as_str().expect("code").to_string();
        let fingerprint = offer["fingerprint"].as_str().expect("fp").to_string();
        let other = DeviceId::generate().as_str().to_string();
        let joiner = thread::spawn(move || {
            join_pairing(
                &address,
                &code,
                &fingerprint,
                "Tablet",
                "android",
                &other,
                Duration::from_secs(60),
            )
        });
        let deadline = Instant::now() + Duration::from_secs(60);
        let mut shown = String::new();
        while shown.is_empty() && Instant::now() < deadline {
            shown = hold_poll().expect("poll");
            thread::sleep(Duration::from_millis(20));
        }
        hold_decide(grant, false).expect("deny");
        assert!(joiner.join().expect("thread").is_err());
        assert_eq!(key_files(&root.0).len(), 1);
    }

    #[test]
    fn object_storage_needs_a_separate_device_key_and_writes_nothing_otherwise() {
        let root = Temp::new("s3");
        let base = serde_json::json!({
            "kind": "r2",
            "endpoint": "http://127.0.0.1:9",
            "bucket": "b",
            "region": "auto",
            "access_key_id": "maintenance",
            "secret_access_key": "m-secret",
        });
        assert!(create_phone_repository(&base.to_string(), &root.0.join("w")).is_err());
        assert_eq!(last_failure(), "fields");
        let mut same = base.clone();
        same["device_access_key_id"] = "maintenance".into();
        same["device_secret_access_key"] = "x".into();
        assert!(create_phone_repository(&same.to_string(), &root.0.join("w")).is_err());
        // Unreachable bucket: the gate fails and nothing is returned to keep.
        let mut full = base;
        full["device_access_key_id"] = "device".into();
        full["device_secret_access_key"] = "d-secret".into();
        assert!(create_phone_repository(&full.to_string(), &root.0.join("w")).is_err());
        assert_eq!(last_failure(), "unreachable");
    }

    #[test]
    fn phone_creates_on_a_bucket_refuses_an_unprotected_one_and_another_device_joins() {
        use super::floci_support::{ACCESS_KEY, LocalFloci, SECRET_KEY, aws};
        let _guard = ONE_AT_A_TIME.lock().unwrap_or_else(|p| p.into_inner());
        let floci = LocalFloci::start();
        let endpoint = floci.endpoint().to_string();
        let bucket = format!("satchel-phone-{}", std::process::id());
        aws(
            &endpoint,
            &["s3api", "create-bucket", "--bucket", bucket.as_str()],
        );
        let root = Temp::new("bucket");
        let spec = serde_json::json!({
            "kind": "b2",
            "endpoint": endpoint,
            "bucket": bucket,
            "region": "us-east-1",
            "prefix": "repo",
            "access_key_id": ACCESS_KEY,
            "secret_access_key": SECRET_KEY,
            "device_access_key_id": "device-key",
            "device_secret_access_key": "device-secret",
        })
        .to_string();

        // Versioning never turned on: refused before anything is written.
        assert!(create_phone_repository(&spec, &root.0.join("work")).is_err());
        assert_eq!(last_failure(), "protection");

        aws(
            &endpoint,
            &[
                "s3api",
                "put-bucket-versioning",
                "--bucket",
                bucket.as_str(),
                "--versioning-configuration",
                "Status=Enabled",
            ],
        );
        let created = create_phone_repository(&spec, &root.0.join("work")).expect("create");
        let (password, grant_text) = created.split_once('\n').expect("two parts");
        let grant = Path::new(grant_text);
        assert!(is_maintainer(grant));
        assert_eq!(catalog_grant(grant).expect("catalog"), "[]");

        let offer = hold_start(grant, "127.0.0.1:0", Duration::from_secs(60)).expect("start");
        let offer: serde_json::Value = serde_json::from_str(&offer).expect("offer");
        let address = offer["address"].as_str().expect("address").to_string();
        let code = offer["code"].as_str().expect("code").to_string();
        let fingerprint = offer["fingerprint"].as_str().expect("fp").to_string();
        let other = DeviceId::generate();
        let other_text = other.as_str().to_string();
        let joiner = thread::spawn(move || {
            join_pairing(
                &address,
                &code,
                &fingerprint,
                "Tablet",
                "android",
                &other_text,
                Duration::from_secs(60),
            )
        });
        let deadline = Instant::now() + Duration::from_secs(60);
        let mut shown = String::new();
        while shown.is_empty() && Instant::now() < deadline {
            shown = hold_poll().expect("poll");
            thread::sleep(Duration::from_millis(20));
        }
        hold_decide(grant, true).expect("approve");
        let joined = joiner.join().expect("thread").expect("joined");
        // The joiner got the bucket, the limited key and its own password, never the rest.
        assert!(joined.contains("device-key"));
        assert!(!joined.contains(password));
        assert!(!joined.contains(&format!("\\\"access_key_id\\\":\\\"{ACCESS_KEY}\\\"")));
        let member_grant = assemble_grant_json(
            &joined,
            other.as_str(),
            root.0.join("member-work").to_str().expect("work"),
        )
        .expect("assemble");
        assert_eq!(
            catalog_grant(Path::new(&member_grant)).expect("member opens"),
            "[]"
        );
        assert!(!is_maintainer(Path::new(&member_grant)));
    }
}
