//! Read bucket versioning and lifecycle from an S3-compatible endpoint.
//!
//! Requests are path-style and signed with SigV4. The AWS config files and the
//! instance metadata service are not used. Failure text does not include keys.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use hmac::Mac;
use sha2::{Digest, Sha256};

use crate::Error;
use crate::storage::Versioning;

const EMPTY_SHA256: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
const BODY_LIMIT: usize = 65_536;
const TIMEOUT: Duration = Duration::from_secs(10);

/// What the bucket API said about recovery of overwritten objects.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ObjectProtection {
    /// Versioning status. `None` is not used: a successful probe always sets it.
    pub versioning: Option<Versioning>,
    /// True when a lifecycle rule can delete an object or a version.
    pub lifecycle_deletes_objects: bool,
}

/// Ask the endpoint whether versioning is on and whether lifecycle deletes objects.
pub fn probe_object_protection(
    endpoint: &str,
    bucket: &str,
    region: &str,
    access_key_id: &str,
    secret_access_key: &str,
) -> Result<ObjectProtection, Error> {
    let auth = ProbeAuth {
        region,
        access_key_id,
        secret_access_key,
    };
    let versioning_body = request(endpoint, "GET", bucket, "", "versioning", &auth)?;
    if versioning_body.status != 200 {
        return Err(Error::Probe);
    }
    let lifecycle = request(endpoint, "GET", bucket, "", "lifecycle", &auth)?;
    let lifecycle_deletes_objects = match lifecycle.status {
        200 => lifecycle_deletes(&lifecycle.body),
        404 => false,
        _ => return Err(Error::Probe),
    };
    Ok(ObjectProtection {
        versioning: Some(versioning_from_body(&versioning_body.body)),
        lifecycle_deletes_objects,
    })
}

/// True when `object_key` exists. A missing object is `Ok(false)`.
pub(crate) fn object_exists(
    endpoint: &str,
    bucket: &str,
    region: &str,
    access_key_id: &str,
    secret_access_key: &str,
    object_key: &str,
) -> Result<bool, Error> {
    let auth = ProbeAuth {
        region,
        access_key_id,
        secret_access_key,
    };
    let response = request(endpoint, "HEAD", bucket, object_key, "", &auth)?;
    match response.status {
        200 => Ok(true),
        404 => Ok(false),
        _ => Err(Error::Probe),
    }
}

struct HttpResponse {
    status: u16,
    body: String,
}

struct Origin {
    https: bool,
    host: String,
    port: u16,
}

struct ProbeAuth<'a> {
    region: &'a str,
    access_key_id: &'a str,
    secret_access_key: &'a str,
}

fn request(
    endpoint: &str,
    method: &str,
    bucket: &str,
    object_key: &str,
    query_name: &str,
    auth: &ProbeAuth<'_>,
) -> Result<HttpResponse, Error> {
    if bucket.is_empty()
        || auth.region.is_empty()
        || auth.access_key_id.is_empty()
        || auth.secret_access_key.is_empty()
    {
        return Err(Error::Probe);
    }
    let origin = parse_origin(endpoint)?;
    let canonical_uri = canonical_path(bucket, object_key)?;
    let canonical_query = if query_name.is_empty() {
        String::new()
    } else {
        format!("{query_name}=")
    };
    let host_header = host_header(&origin);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| Error::Probe)?
        .as_secs();
    let (date, amz_date) = utc_stamp(now);
    let authorization = sign(&SignInput {
        method,
        canonical_uri: &canonical_uri,
        canonical_query: &canonical_query,
        host_header: &host_header,
        date: &date,
        amz_date: &amz_date,
        auth,
    })?;
    let query_suffix = if canonical_query.is_empty() {
        String::new()
    } else {
        format!("?{query_name}")
    };
    let message = format!(
        "{method} {canonical_uri}{query_suffix} HTTP/1.1\r\nHost: {host_header}\r\nx-amz-date: {amz_date}\r\nx-amz-content-sha256: {EMPTY_SHA256}\r\nAuthorization: {authorization}\r\nConnection: close\r\n\r\n"
    );
    let bytes = exchange(&origin, message.as_bytes())?;
    parse_response(&bytes)
}

struct SignInput<'a> {
    method: &'a str,
    canonical_uri: &'a str,
    canonical_query: &'a str,
    host_header: &'a str,
    date: &'a str,
    amz_date: &'a str,
    auth: &'a ProbeAuth<'a>,
}

fn sign(input: &SignInput<'_>) -> Result<String, Error> {
    let canonical_headers = format!(
        "host:{}\nx-amz-content-sha256:{EMPTY_SHA256}\nx-amz-date:{}\n",
        input.host_header, input.amz_date
    );
    let signed_headers = "host;x-amz-content-sha256;x-amz-date";
    let canonical = format!(
        "{}\n{}\n{}\n{canonical_headers}\n{signed_headers}\n{EMPTY_SHA256}",
        input.method, input.canonical_uri, input.canonical_query
    );
    let scope = format!("{}/{}/s3/aws4_request", input.date, input.auth.region);
    let hash = hex::encode(Sha256::digest(canonical.as_bytes()));
    let string_to_sign = format!("AWS4-HMAC-SHA256\n{}\n{scope}\n{hash}", input.amz_date);
    let k_date = hmac_sha256(
        format!("AWS4{}", input.auth.secret_access_key).as_bytes(),
        input.date.as_bytes(),
    )?;
    let k_region = hmac_sha256(&k_date, input.auth.region.as_bytes())?;
    let k_service = hmac_sha256(&k_region, b"s3")?;
    let k_signing = hmac_sha256(&k_service, b"aws4_request")?;
    let signature = hex::encode(hmac_sha256(&k_signing, string_to_sign.as_bytes())?);
    Ok(format!(
        "AWS4-HMAC-SHA256 Credential={}/{scope}, SignedHeaders={signed_headers}, Signature={signature}",
        input.auth.access_key_id
    ))
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> Result<Vec<u8>, Error> {
    let mut mac = hmac::Hmac::<Sha256>::new_from_slice(key).map_err(|_| Error::Probe)?;
    mac.update(data);
    Ok(mac.finalize().into_bytes().to_vec())
}

fn canonical_path(bucket: &str, object_key: &str) -> Result<String, Error> {
    let mut path = format!("/{}", encode_segment(bucket)?);
    if !object_key.is_empty() {
        for segment in object_key.split('/') {
            path.push('/');
            path.push_str(&encode_segment(segment)?);
        }
    }
    Ok(path)
}

fn encode_segment(segment: &str) -> Result<String, Error> {
    if segment.is_empty() {
        return Err(Error::Probe);
    }
    let mut encoded = String::new();
    for byte in segment.bytes() {
        let unreserved = byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~');
        if unreserved {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    Ok(encoded)
}

fn parse_origin(endpoint: &str) -> Result<Origin, Error> {
    let (https, rest) = if let Some(rest) = endpoint.strip_prefix("https://") {
        (true, rest)
    } else if let Some(rest) = endpoint.strip_prefix("http://") {
        (false, rest)
    } else {
        return Err(Error::Probe);
    };
    if rest.is_empty() || rest.contains('/') || rest.contains('?') || rest.contains('@') {
        return Err(Error::Probe);
    }
    let (host, port) = if let Some(host) = rest.strip_prefix('[') {
        let Some((host, port_text)) = host.split_once("]:") else {
            return Err(Error::Probe);
        };
        (host.to_string(), parse_port(port_text)?)
    } else if let Some((host, port_text)) = rest.rsplit_once(':') {
        if host.contains(':') {
            return Err(Error::Probe);
        }
        (host.to_string(), parse_port(port_text)?)
    } else if https {
        (rest.to_string(), 443)
    } else {
        (rest.to_string(), 80)
    };
    if host.is_empty() {
        return Err(Error::Probe);
    }
    Ok(Origin { https, host, port })
}

fn parse_port(text: &str) -> Result<u16, Error> {
    text.parse::<u16>().map_err(|_| Error::Probe)
}

fn host_header(origin: &Origin) -> String {
    let default_port = if origin.https { 443 } else { 80 };
    if origin.port == default_port {
        return origin.host.clone();
    }
    format!("{}:{}", origin.host, origin.port)
}

fn exchange(origin: &Origin, request: &[u8]) -> Result<Vec<u8>, Error> {
    let address = format!("{}:{}", origin.host, origin.port);
    let mut stream = TcpStream::connect(address).map_err(|_| Error::Probe)?;
    stream
        .set_read_timeout(Some(TIMEOUT))
        .map_err(|_| Error::Probe)?;
    stream
        .set_write_timeout(Some(TIMEOUT))
        .map_err(|_| Error::Probe)?;
    if origin.https {
        return exchange_tls(origin, &mut stream, request);
    }
    stream.write_all(request).map_err(|_| Error::Probe)?;
    read_limited(&mut stream)
}

fn exchange_tls(origin: &Origin, stream: &mut TcpStream, request: &[u8]) -> Result<Vec<u8>, Error> {
    let mut roots = rustls::RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let config = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let name = rustls::pki_types::ServerName::try_from(origin.host.as_str())
        .map_err(|_| Error::Probe)?
        .to_owned();
    let mut connection =
        rustls::ClientConnection::new(Arc::new(config), name).map_err(|_| Error::Probe)?;
    let mut tls = rustls::Stream::new(&mut connection, stream);
    tls.write_all(request).map_err(|_| Error::Probe)?;
    read_limited(&mut tls)
}

fn read_limited(stream: &mut impl Read) -> Result<Vec<u8>, Error> {
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 2048];
    while buffer.len() <= BODY_LIMIT + 8192 {
        let read = stream.read(&mut chunk).map_err(|_| Error::Probe)?;
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);
    }
    if buffer.len() > BODY_LIMIT + 8192 {
        return Err(Error::Probe);
    }
    Ok(buffer)
}

fn parse_response(bytes: &[u8]) -> Result<HttpResponse, Error> {
    let header_end = find_separator(bytes).ok_or(Error::Probe)?;
    let header_text = std::str::from_utf8(&bytes[..header_end]).map_err(|_| Error::Probe)?;
    let mut lines = header_text.split("\r\n");
    let status_line = lines.next().ok_or(Error::Probe)?;
    let mut parts = status_line.split(' ');
    let _http = parts.next().ok_or(Error::Probe)?;
    let status = parts
        .next()
        .ok_or(Error::Probe)?
        .parse::<u16>()
        .map_err(|_| Error::Probe)?;
    let mut length = None;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        if name.eq_ignore_ascii_case("content-length") {
            length = Some(value.trim().parse::<usize>().map_err(|_| Error::Probe)?);
        }
    }
    let body_start = header_end + 4;
    let body_bytes = if let Some(length) = length {
        if length > BODY_LIMIT || body_start + length > bytes.len() {
            return Err(Error::Probe);
        }
        &bytes[body_start..body_start + length]
    } else if bytes.len() - body_start > BODY_LIMIT {
        return Err(Error::Probe);
    } else {
        &bytes[body_start..]
    };
    let body = String::from_utf8_lossy(body_bytes).into_owned();
    Ok(HttpResponse { status, body })
}

fn find_separator(bytes: &[u8]) -> Option<usize> {
    let mut index = 0;
    while index + 3 < bytes.len() {
        if bytes[index] == b'\r'
            && bytes[index + 1] == b'\n'
            && bytes[index + 2] == b'\r'
            && bytes[index + 3] == b'\n'
        {
            return Some(index);
        }
        index += 1;
    }
    None
}

fn versioning_from_body(body: &str) -> Versioning {
    if body.contains("<Status>Enabled</Status>") || body.contains("\"Status\":\"Enabled\"") {
        return Versioning::Enabled;
    }
    if body.contains("<Status>Suspended</Status>") || body.contains("\"Status\":\"Suspended\"") {
        return Versioning::Suspended;
    }
    Versioning::Off
}

fn lifecycle_deletes(body: &str) -> bool {
    body.contains("<Expiration")
        || body.contains("<NoncurrentVersionExpiration")
        || body.contains("<ExpiredObjectDeleteMarker")
}

fn utc_stamp(seconds: u64) -> (String, String) {
    let days = (seconds / 86_400) as i64;
    let time = seconds % 86_400;
    let hour = time / 3600;
    let minute = (time % 3600) / 60;
    let second = time % 60;
    let (year, month, day) = civil_from_days(days);
    let date = format!("{year:04}{month:02}{day:02}");
    let stamp = format!("{date}T{hour:02}{minute:02}{second:02}Z");
    (date, stamp)
}

/// Howard Hinnant's civil_from_days, days since 1970-01-01.
fn civil_from_days(days: i64) -> (i32, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    let year = if month <= 2 { y + 1 } else { y };
    (year as i32, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utc_stamp_formats_the_controlled_clock() {
        let (date, stamp) = utc_stamp(1_700_000_000);
        assert_eq!(date, "20231114");
        assert_eq!(stamp, "20231114T221320Z");
    }

    #[test]
    fn versioning_body_maps_the_three_states() {
        assert_eq!(
            versioning_from_body("<Status>Enabled</Status>"),
            Versioning::Enabled
        );
        assert_eq!(
            versioning_from_body("<Status>Suspended</Status>"),
            Versioning::Suspended
        );
        assert_eq!(
            versioning_from_body("<VersioningConfiguration/>"),
            Versioning::Off
        );
    }

    #[test]
    fn lifecycle_deletion_is_only_the_deleting_rules() {
        assert!(!lifecycle_deletes("<LifecycleConfiguration/>"));
        assert!(lifecycle_deletes("<Expiration><Days>1</Days></Expiration>"));
        assert!(lifecycle_deletes(
            "<NoncurrentVersionExpiration><NoncurrentDays>1</NoncurrentDays></NoncurrentVersionExpiration>"
        ));
    }

    #[test]
    fn origin_keeps_the_explicit_port() {
        let origin = parse_origin("http://127.0.0.1:4566").expect("origin");
        assert!(!origin.https);
        assert_eq!(origin.host, "127.0.0.1");
        assert_eq!(origin.port, 4566);
        assert_eq!(host_header(&origin), "127.0.0.1:4566");
        assert!(parse_origin("http://127.0.0.1:4566/bucket").is_err());
    }
}
