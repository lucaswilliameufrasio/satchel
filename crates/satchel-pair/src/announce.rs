use std::net::IpAddr;

use mdns_sd::ServiceInfo;
use serde::{Deserialize, Serialize};

use crate::error::Error;
use crate::util::{hex_decode_32, random_hex};

/// DNS-SD type. The port rides in the SRV record, not in TXT.
pub const SERVICE_TYPE: &str = "_satchel._tcp.local.";

/// TXT version. Independent of the restic repository.
pub const ANNOUNCE_VERSION: &str = "1";

const TXT_KEYS: [&str; 3] = ["alias", "fp", "v"];

/// Invite the joiner can type or, later, put in a QR image.
///
/// The fields are the code, the address to dial, and the certificate fingerprint.
/// The recovery password is not a field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Invite {
    /// Protocol version.
    pub v: u8,
    /// Six digits, without a space.
    pub code: String,
    /// `host:port` the joiner dials. This is not a discovered subnet.
    pub address: String,
    /// SHA-256 of the certificate DER, lowercase hex.
    pub fingerprint: String,
}

impl Invite {
    /// Build the invite the add-device screen shows.
    pub fn new(
        code_digits: &str,
        address: impl Into<String>,
        fingerprint: &str,
    ) -> Result<Self, Error> {
        let invite = Self {
            v: 1,
            code: code_digits.to_string(),
            address: address.into(),
            fingerprint: fingerprint.to_string(),
        };
        invite.check()?;
        Ok(invite)
    }

    /// JSON object with only `v`, `code`, `address`, and `fingerprint`.
    pub fn encode(&self) -> Result<String, Error> {
        self.check()?;
        serde_json::to_string(self).map_err(|_| Error::Message)
    }

    /// Reject unknown fields, including anything that would carry a secret.
    pub fn decode(text: &str) -> Result<Self, Error> {
        let invite: Self = serde_json::from_str(text).map_err(|_| Error::Message)?;
        invite.check()?;
        Ok(invite)
    }

    fn check(&self) -> Result<(), Error> {
        if self.v != 1 {
            return Err(Error::Version);
        }
        if self.code.len() != 6 {
            return Err(Error::Code);
        }
        for character in self.code.chars() {
            if !character.is_ascii_digit() {
                return Err(Error::Code);
            }
        }
        if self.address.is_empty()
            || self.address.len() > 256
            || self.address.chars().any(|c| c.is_control())
        {
            return Err(Error::Message);
        }
        hex_decode_32(&self.fingerprint)?;
        Ok(())
    }
}

/// Random session alias. It is not a hostname and not a repository id.
pub fn random_alias() -> String {
    random_hex(8)
}

/// Multicast stays off loopback, mesh tunnels, and container bridges.
pub fn is_local_multicast_interface(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    let lower = name.to_ascii_lowercase();
    if lower == "lo" {
        return false;
    }
    if let Some(rest) = lower.strip_prefix("lo")
        && !rest.is_empty()
        && rest.chars().all(|character| character.is_ascii_digit())
    {
        return false;
    }
    const DENY: &[&str] = &[
        "tailscale",
        "utun",
        "wg",
        "tun",
        "tap",
        "zt",
        "docker",
        "br-",
        "veth",
        "virbr",
        "nebula",
        "netbird",
        "cni",
        "flannel",
        "cali",
        "lxc",
        "vnet",
        "wt",
        "vpn",
    ];
    for prefix in DENY {
        if lower.starts_with(prefix) {
            return false;
        }
    }
    true
}

/// The record that would be advertised. TXT keys are exactly version, alias, and fingerprint.
pub fn announcement_record(
    alias: &str,
    port: u16,
    fingerprint: &str,
    addresses: &[IpAddr],
) -> Result<ServiceInfo, Error> {
    if alias.is_empty()
        || alias.len() > 63
        || alias
            .chars()
            .any(|character| !character.is_ascii_hexdigit())
    {
        return Err(Error::Message);
    }
    hex_decode_32(fingerprint)?;
    if addresses.is_empty() || port == 0 {
        return Err(Error::Message);
    }
    let host = format!("{alias}.local.");
    let properties = [
        ("v", ANNOUNCE_VERSION),
        ("alias", alias),
        ("fp", fingerprint),
    ];
    let info = ServiceInfo::new(SERVICE_TYPE, alias, &host, addresses, port, &properties[..])
        .map_err(|_| Error::Message)?;
    let mut keys = Vec::new();
    for property in info.get_properties().iter() {
        keys.push(property.key().to_string());
    }
    keys.sort();
    let mut expected = Vec::new();
    for key in TXT_KEYS {
        expected.push(key.to_string());
    }
    if keys != expected {
        return Err(Error::Message);
    }
    Ok(info)
}

/// Name of the interface that owns `ip`, from the local address table.
pub fn interface_for(ip: IpAddr) -> String {
    let Ok(list) = if_addrs::get_if_addrs() else {
        return "unknown".to_string();
    };
    for iface in list {
        if iface.ip() == ip {
            return iface.name;
        }
    }
    "unknown".to_string()
}

#[cfg(test)]
mod tests {
    use std::net::Ipv4Addr;

    use super::{Invite, announcement_record, is_local_multicast_interface};

    #[test]
    fn announcement_txt_is_only_version_alias_and_fingerprint() {
        let fingerprint = "ab".repeat(32);
        let info = announcement_record(
            "a1b2c3d4e5f60718",
            4321,
            &fingerprint,
            &[Ipv4Addr::new(192, 0, 2, 10).into()],
        )
        .expect("record");
        let mut keys = Vec::new();
        for property in info.get_properties().iter() {
            keys.push(property.key().to_string());
        }
        keys.sort();
        assert_eq!(
            keys,
            vec!["alias".to_string(), "fp".to_string(), "v".to_string()]
        );
        assert_eq!(info.get_property_val_str("v"), Some("1"));
        assert_eq!(info.get_property_val_str("alias"), Some("a1b2c3d4e5f60718"));
        assert_eq!(info.get_property_val_str("fp"), Some(fingerprint.as_str()));
        assert_eq!(info.get_port(), 4321);
        assert!(info.get_fullname().contains("_satchel._tcp.local."));
        assert!(info.get_property_val_str("bucket").is_none());
        assert!(info.get_property_val_str("repo").is_none());
    }

    #[test]
    fn multicast_skips_mesh_loopback_and_bridges() {
        assert!(is_local_multicast_interface("eth0"));
        assert!(is_local_multicast_interface("enp0s3"));
        assert!(is_local_multicast_interface("wlan0"));
        assert!(is_local_multicast_interface("wlp2s0"));
        assert!(!is_local_multicast_interface("lo"));
        assert!(!is_local_multicast_interface("lo0"));
        assert!(!is_local_multicast_interface("tailscale0"));
        assert!(!is_local_multicast_interface("wt0"));
        assert!(!is_local_multicast_interface("wg0"));
        assert!(!is_local_multicast_interface("docker0"));
        assert!(!is_local_multicast_interface("veth12"));
        assert!(!is_local_multicast_interface("br-abcd"));
        assert!(!is_local_multicast_interface(""));
    }

    #[test]
    fn invite_json_has_no_room_for_a_recovery_password() {
        let fingerprint = "cd".repeat(32);
        let invite = Invite::new("482193", "127.0.0.1:9", &fingerprint).expect("invite");
        let text = invite.encode().expect("json");
        let parsed: serde_json::Value = serde_json::from_str(&text).expect("value");
        let object = parsed.as_object().expect("object");
        let mut keys = Vec::new();
        for key in object.keys() {
            keys.push(key.clone());
        }
        keys.sort();
        assert_eq!(
            keys,
            vec![
                "address".to_string(),
                "code".to_string(),
                "fingerprint".to_string(),
                "v".to_string()
            ]
        );
        assert!(!text.contains("recovery"));
        let back = Invite::decode(&text).expect("decode");
        assert_eq!(back, invite);
        let mut extra = parsed;
        extra.as_object_mut().expect("object").insert(
            "recovery".to_string(),
            serde_json::Value::String("no".to_string()),
        );
        let extra_text = serde_json::to_string(&extra).expect("extra");
        assert!(extra_text.contains(&fingerprint));
        assert!(Invite::decode(&extra_text).is_err());
    }
}
