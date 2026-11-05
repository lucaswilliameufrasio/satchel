use crate::Error;

pub(crate) fn hex_encode(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        text.push_str(&format!("{byte:02x}"));
    }
    text
}

pub(crate) fn hex_decode_32(text: &str) -> Result<[u8; 32], Error> {
    if text.len() != 64 {
        return Err(Error::Fingerprint);
    }
    let mut out = [0_u8; 32];
    for (index, slot) in out.iter_mut().enumerate() {
        let start = index * 2;
        let byte =
            u8::from_str_radix(&text[start..start + 2], 16).map_err(|_| Error::Fingerprint)?;
        *slot = byte;
    }
    Ok(out)
}

pub(crate) fn random_hex(bytes_len: usize) -> String {
    use rand::Rng;

    let mut bytes = vec![0_u8; bytes_len];
    rand::rng().fill_bytes(&mut bytes);
    hex_encode(&bytes)
}

/// The IPv4 address another device can dial: the mesh range first, then a private network.
///
/// Loopback, link-local, container bridges, and anything public are skipped. `None`
/// means no interface qualifies and the person has to type the address.
pub fn preferred_bind_ip() -> Option<std::net::IpAddr> {
    let (mesh, private) = candidate_ips()?;
    mesh.or(private).map(std::net::IpAddr::V4)
}

/// This machine's IPv4 address on a Tailscale or NetBird network (100.64.0.0/10).
///
/// `None` when no such interface is up. A LAN address is never returned.
pub fn mesh_ip() -> Option<std::net::IpAddr> {
    candidate_ips()?.0.map(std::net::IpAddr::V4)
}

fn candidate_ips() -> Option<(Option<std::net::Ipv4Addr>, Option<std::net::Ipv4Addr>)> {
    let list = if_addrs::get_if_addrs().ok()?;
    let mut mesh = None;
    let mut private = None;
    for iface in list {
        let std::net::IpAddr::V4(ip) = iface.ip() else {
            continue;
        };
        if iface.is_loopback() || ip.is_link_local() || ip.is_unspecified() {
            continue;
        }
        let name = iface.name.to_ascii_lowercase();
        if name.starts_with("docker") || name.starts_with("br-") || name.starts_with("veth") {
            continue;
        }
        let octets = ip.octets();
        let carrier_grade = octets[0] == 100 && (64..128).contains(&octets[1]);
        if carrier_grade && mesh.is_none() {
            mesh = Some(ip);
        } else if ip.is_private() && private.is_none() {
            private = Some(ip);
        }
    }
    Some((mesh, private))
}
