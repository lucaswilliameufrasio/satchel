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
