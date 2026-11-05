//! The key and port this desktop listens on for a wake notice.
//!
//! Both stay in the config directory, readable only by the user. The key is not
//! a storage secret: whoever holds it can only make this desktop list
//! `snapshots/` sooner. The port is kept so peers that read an older snapshot
//! still reach this desktop after a restart.

use std::net::{IpAddr, SocketAddr};
use std::path::Path;

use satchel_core::{WakeEndpoint, WakeListener, generate_wake_key};
use serde::{Deserialize, Serialize};

use crate::Error;

const FILE: &str = "wake.json";

#[derive(Deserialize, Serialize)]
struct Saved {
    key: String,
    port: u16,
}

fn load(config_dir: &Path) -> Option<Saved> {
    let bytes = std::fs::read(config_dir.join(FILE)).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn save(config_dir: &Path, saved: &Saved) -> Result<(), Error> {
    let bytes = serde_json::to_vec(saved).map_err(|_| Error::Io)?;
    let temp = config_dir.join(".wake.json.tmp");
    std::fs::write(&temp, bytes).map_err(|_| Error::Io)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&temp, std::fs::Permissions::from_mode(0o600))
            .map_err(|_| Error::Io)?;
    }
    std::fs::rename(&temp, config_dir.join(FILE)).map_err(|_| Error::Io)
}

/// Bind on `ip`, reusing the saved port when it is free.
pub(crate) fn bind(config_dir: &Path, ip: IpAddr) -> Result<(WakeListener, WakeEndpoint), Error> {
    let saved = load(config_dir);
    let key = match &saved {
        Some(saved) => saved.key.clone(),
        None => generate_wake_key(),
    };
    let mut listener = None;
    if let Some(saved) = &saved {
        listener = WakeListener::bind(SocketAddr::new(ip, saved.port), &key).ok();
    }
    let listener = match listener {
        Some(listener) => listener,
        None => WakeListener::bind(SocketAddr::new(ip, 0), &key)?,
    };
    let address = listener.local_addr()?;
    if saved.as_ref().map(|saved| saved.port) != Some(address.port()) {
        save(
            config_dir,
            &Saved {
                key: key.clone(),
                port: address.port(),
            },
        )?;
    }
    Ok((
        listener,
        WakeEndpoint {
            address: address.to_string(),
            key,
        },
    ))
}
