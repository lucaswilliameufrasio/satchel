//! A "look now" notice between paired devices on the same mesh network.
//!
//! A device that listens publishes its mesh address and a random key of its
//! own inside its encrypted snapshot, so only devices that open the repository
//! learn them. After a sync that wrote a snapshot, a device sends each listening
//! peer one UDP datagram signed with that peer's key. The datagram carries no
//! data and no path: the peer only runs its cheap check sooner. A lost or forged
//! datagram costs at most one listing of `snapshots/`, and the timer still runs.
//!
//! Datagram, 96 bytes: `SATWAKE1`, the send time as big-endian Unix seconds,
//! 16 random bytes, the sender's 32-character device id, and an HMAC-SHA256 of
//! everything before it under the receiver's key.

use std::collections::BTreeMap;
use std::net::{IpAddr, SocketAddr, UdpSocket};
use std::path::Path;
use std::time::Duration;

use hmac::{Hmac, Mac};
use rand::Rng;
use serde::{Deserialize, Serialize};
use sha2::Sha256;

use crate::Error;
use crate::format::DeviceId;

const MAGIC: &[u8; 8] = b"SATWAKE1";
const SIGNED_LEN: usize = 8 + 8 + 16 + 32;
const DATAGRAM_LEN: usize = SIGNED_LEN + 32;
/// A notice older or newer than this is dropped, so a captured one cannot be replayed later.
pub const WAKE_WINDOW_SECS: u64 = 300;

/// Where a device listens and the key a notice to it must be signed with.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct WakeEndpoint {
    /// `ip:port` on the mesh network.
    pub address: String,
    /// 32 bytes, hex.
    pub key: String,
}

/// A listening peer, read from its latest snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WakePeer {
    /// The peer's device id.
    pub device_id: String,
    /// Where to send the notice.
    pub endpoint: WakeEndpoint,
}

/// A fresh random key for [`WakeEndpoint::key`].
pub fn generate_wake_key() -> String {
    let mut bytes = [0_u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

fn key_bytes(key: &str) -> Result<[u8; 32], Error> {
    let decoded = hex::decode(key).map_err(|_| Error::Wake)?;
    decoded.try_into().map_err(|_| Error::Wake)
}

fn tag(key: &[u8; 32], signed: &[u8]) -> Hmac<Sha256> {
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(key).expect("any key length");
    mac.update(signed);
    mac
}

/// The datagram that asks the holder of `key` to look now.
pub fn wake_datagram(
    key: &str,
    sender: &DeviceId,
    unix: u64,
    nonce: [u8; 16],
) -> Result<Vec<u8>, Error> {
    let key = key_bytes(key)?;
    let id = sender.as_str().as_bytes();
    if id.len() != 32 {
        return Err(Error::Wake);
    }
    let mut out = Vec::with_capacity(DATAGRAM_LEN);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&unix.to_be_bytes());
    out.extend_from_slice(&nonce);
    out.extend_from_slice(id);
    let code = tag(&key, &out).finalize().into_bytes();
    out.extend_from_slice(&code);
    Ok(out)
}

/// Send one notice to each peer. Returns how many datagrams left this device.
///
/// A send that fails is skipped: the peer's timer covers it.
pub fn send_wakes(sender: &DeviceId, peers: &[WakePeer], unix: u64) -> usize {
    let Ok(socket) = UdpSocket::bind(("0.0.0.0", 0)) else {
        return 0;
    };
    let mut sent = 0;
    for peer in peers {
        if peer.device_id == sender.as_str() {
            continue;
        }
        let Ok(address) = peer.endpoint.address.parse::<SocketAddr>() else {
            continue;
        };
        let mut nonce = [0_u8; 16];
        rand::rng().fill_bytes(&mut nonce);
        let Ok(datagram) = wake_datagram(&peer.endpoint.key, sender, unix, nonce) else {
            continue;
        };
        if socket.send_to(&datagram, address).is_ok() {
            sent += 1;
        }
    }
    sent
}

/// Checks notices against this device's key and drops replays.
pub struct WakeCheck {
    key: [u8; 32],
    /// Nonces accepted inside the window, with their send time.
    seen: BTreeMap<[u8; 16], u64>,
}

impl WakeCheck {
    /// Check notices signed with `key`.
    pub fn new(key: &str) -> Result<Self, Error> {
        Ok(Self {
            key: key_bytes(key)?,
            seen: BTreeMap::new(),
        })
    }

    /// The sender's device id when `datagram` is a fresh notice signed with this key.
    pub fn accept(&mut self, datagram: &[u8], now: u64) -> Option<String> {
        if datagram.len() != DATAGRAM_LEN || &datagram[..8] != MAGIC {
            return None;
        }
        let (signed, code) = datagram.split_at(SIGNED_LEN);
        if tag(&self.key, signed).verify_slice(code).is_err() {
            return None;
        }
        let unix = u64::from_be_bytes(signed[8..16].try_into().ok()?);
        if unix.abs_diff(now) > WAKE_WINDOW_SECS {
            return None;
        }
        let nonce: [u8; 16] = signed[16..32].try_into().ok()?;
        self.seen
            .retain(|_, sent| sent.abs_diff(now) <= WAKE_WINDOW_SECS);
        if self.seen.contains_key(&nonce) {
            return None;
        }
        self.seen.insert(nonce, unix);
        let sender = std::str::from_utf8(&signed[32..64]).ok()?;
        DeviceId::parse(sender)
            .ok()
            .map(|id| id.as_str().to_string())
    }
}

#[derive(Deserialize, Serialize)]
struct SavedWake {
    key: String,
    port: u16,
}

/// Bind on `ip` with the key and port kept in `file`, creating them on first use.
///
/// The saved port is reused when it is free, so peers that read an older
/// snapshot still reach this device after a restart. `file` should be readable
/// only by the user; it is written with mode 0600 on Unix. The key is not a
/// storage secret: whoever holds it can only make this device list
/// `snapshots/` sooner.
pub fn bind_saved_wake(file: &Path, ip: IpAddr) -> Result<(WakeListener, WakeEndpoint), Error> {
    let saved: Option<SavedWake> = std::fs::read(file)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok());
    let key = match &saved {
        Some(saved) if key_bytes(&saved.key).is_ok() => saved.key.clone(),
        _ => generate_wake_key(),
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
    let unchanged = saved
        .as_ref()
        .is_some_and(|saved| saved.port == address.port() && saved.key == key);
    if !unchanged {
        let bytes = serde_json::to_vec(&SavedWake {
            key: key.clone(),
            port: address.port(),
        })
        .map_err(|_| Error::Wake)?;
        write_private(file, &bytes)?;
    }
    Ok((
        listener,
        WakeEndpoint {
            address: address.to_string(),
            key,
        },
    ))
}

fn write_private(file: &Path, bytes: &[u8]) -> Result<(), Error> {
    let name = file
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or(Error::Wake)?;
    let temp = file.with_file_name(format!(".{name}.tmp"));
    std::fs::write(&temp, bytes).map_err(|_| Error::Wake)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&temp, std::fs::Permissions::from_mode(0o600))
            .map_err(|_| Error::Wake)?;
    }
    std::fs::rename(&temp, file).map_err(|_| Error::Wake)
}

/// A bound socket that waits for notices.
pub struct WakeListener {
    socket: UdpSocket,
    check: WakeCheck,
}

impl WakeListener {
    /// Listen on `address`. Port 0 picks a free port; read it back with [`Self::local_addr`].
    pub fn bind(address: SocketAddr, key: &str) -> Result<Self, Error> {
        let socket = UdpSocket::bind(address).map_err(|_| Error::Wake)?;
        Ok(Self {
            socket,
            check: WakeCheck::new(key)?,
        })
    }

    /// The address the socket holds.
    pub fn local_addr(&self) -> Result<SocketAddr, Error> {
        self.socket.local_addr().map_err(|_| Error::Wake)
    }

    /// Wait up to `timeout` for one valid notice. Returns the sender's device id.
    ///
    /// Invalid datagrams are dropped and the wait goes on until the timeout.
    pub fn wait(&mut self, timeout: Duration, now: impl Fn() -> u64) -> Option<String> {
        let deadline = std::time::Instant::now() + timeout;
        let mut buffer = [0_u8; DATAGRAM_LEN + 1];
        loop {
            let left = deadline.checked_duration_since(std::time::Instant::now())?;
            if left.is_zero() || self.socket.set_read_timeout(Some(left)).is_err() {
                return None;
            }
            let Ok((read, _)) = self.socket.recv_from(&mut buffer) else {
                return None;
            };
            if let Some(sender) = self.check.accept(&buffer[..read], now()) {
                return Some(sender);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SENDER: &str = "0123456789abcdef0123456789abcdef";

    fn sender() -> DeviceId {
        DeviceId::parse(SENDER).expect("id")
    }

    #[test]
    fn a_signed_fresh_notice_is_accepted_once() {
        let key = generate_wake_key();
        let mut check = WakeCheck::new(&key).expect("check");
        let datagram = wake_datagram(&key, &sender(), 1_000, [7; 16]).expect("datagram");
        assert_eq!(datagram.len(), DATAGRAM_LEN);
        assert_eq!(check.accept(&datagram, 1_010).as_deref(), Some(SENDER));
        assert_eq!(check.accept(&datagram, 1_011), None, "a replay is dropped");
        let other = wake_datagram(&key, &sender(), 1_000, [8; 16]).expect("datagram");
        assert_eq!(check.accept(&other, 1_012).as_deref(), Some(SENDER));
    }

    #[test]
    fn a_wrong_key_a_stale_time_or_a_changed_byte_is_dropped() {
        let key = generate_wake_key();
        let mut check = WakeCheck::new(&key).expect("check");
        let foreign = wake_datagram(&generate_wake_key(), &sender(), 1_000, [1; 16]).expect("d");
        assert_eq!(check.accept(&foreign, 1_000), None);
        let stale = wake_datagram(&key, &sender(), 1_000, [2; 16]).expect("d");
        assert_eq!(check.accept(&stale, 1_000 + WAKE_WINDOW_SECS + 1), None);
        assert_eq!(check.accept(&stale, 1_000 - WAKE_WINDOW_SECS - 1), None);
        let mut changed = wake_datagram(&key, &sender(), 1_000, [3; 16]).expect("d");
        changed[40] ^= 1;
        assert_eq!(check.accept(&changed, 1_000), None);
        let good = wake_datagram(&key, &sender(), 1_000, [4; 16]).expect("d");
        assert_eq!(check.accept(&good[..good.len() - 1], 1_000), None);
        assert_eq!(check.accept(b"", 1_000), None);
        assert!(WakeCheck::new("not hex").is_err());
        assert!(WakeCheck::new("abcd").is_err());
    }

    #[test]
    fn a_notice_crosses_a_socket() {
        let key = generate_wake_key();
        let mut listener =
            WakeListener::bind("127.0.0.1:0".parse().expect("addr"), &key).expect("bind");
        let address = listener.local_addr().expect("addr").to_string();
        let peers = vec![
            WakePeer {
                device_id: "fedcba9876543210fedcba9876543210".to_string(),
                endpoint: WakeEndpoint {
                    address: address.clone(),
                    key: key.clone(),
                },
            },
            WakePeer {
                device_id: SENDER.to_string(),
                endpoint: WakeEndpoint {
                    address,
                    key: key.clone(),
                },
            },
        ];
        assert_eq!(send_wakes(&sender(), &peers, 5_000), 1, "never to itself");
        let woke = listener.wait(Duration::from_secs(2), || 5_000);
        assert_eq!(woke.as_deref(), Some(SENDER));
        assert_eq!(listener.wait(Duration::from_millis(100), || 5_000), None);
    }
}
