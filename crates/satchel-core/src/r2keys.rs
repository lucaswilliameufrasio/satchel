//! One bucket key per device on Cloudflare R2, through the Cloudflare API.
//!
//! R2's S3 keys are Cloudflare API tokens: the access key id is the token id and
//! the secret is the SHA-256 of the token value. A Cloudflare token allowed to
//! create tokens (kept apart from the S3 maintenance key) makes one token per
//! device, scoped to the repository's bucket with "Workers R2 Storage Bucket Item
//! Write". R2 can't scope a token to a prefix: the device key reaches the whole
//! bucket. Failure text does not include tokens.

use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::Error;
use crate::iam::DeviceBucketKey;
use crate::probe::{exchange, host_header, parse_origin, parse_response};

/// Where the Cloudflare API answers. Tests point it elsewhere.
pub const CLOUDFLARE_API: &str = "https://api.cloudflare.com";

const WRITE_GROUP: &str = "Workers R2 Storage Bucket Item Write";

/// Account id and jurisdiction from an R2 endpoint such as
/// `https://<account>.r2.cloudflarestorage.com` or `https://<account>.eu.r2.cloudflarestorage.com`.
pub fn r2_account(endpoint: &str) -> Option<(String, String)> {
    let host = endpoint
        .split("://")
        .nth(1)
        .unwrap_or(endpoint)
        .split(['/', ':'])
        .next()?
        .to_ascii_lowercase();
    let rest = host.strip_suffix(".r2.cloudflarestorage.com")?;
    let mut parts = rest.split('.');
    let account = parts.next()?.to_string();
    let jurisdiction = parts.next().unwrap_or("default").to_string();
    if account.is_empty() || !account.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
        return None;
    }
    Some((account, jurisdiction))
}

/// Create the device's token, scoped to the bucket, as S3 credentials.
pub fn issue_r2_device_key(
    api: &str,
    token: &str,
    endpoint: &str,
    bucket: &str,
    prefix: &str,
    device_id: &str,
) -> Result<DeviceBucketKey, Error> {
    let name = crate::device_key_user(bucket, prefix, device_id)?;
    let (account, jurisdiction) = r2_account(endpoint).ok_or(Error::Keys)?;
    // A token left by an earlier attempt is replaced, not kept beside the new one.
    delete_named(api, token, &name)?;
    let group = permission_group(api, token)?;
    let resource = format!("com.cloudflare.edge.r2.bucket.{account}_{jurisdiction}_{bucket}");
    let created = call(
        api,
        token,
        "POST",
        "/client/v4/user/tokens",
        Some(&json!({
            "name": name,
            "policies": [{
                "effect": "allow",
                "resources": { resource: "*" },
                "permission_groups": [{ "id": group }],
            }],
        })),
    )?;
    let field = |key: &str| {
        created
            .get(key)
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or(Error::Keys)
    };
    let value = field("value")?;
    Ok(DeviceBucketKey {
        user_name: name,
        access_key_id: field("id")?,
        secret_access_key: hex::encode(Sha256::digest(value.as_bytes())),
    })
}

/// Delete the device's token. A token that is already gone counts as removed.
pub fn revoke_r2_device_key(api: &str, token: &str, name: &str) -> Result<(), Error> {
    if !name.starts_with("satchel-") {
        return Err(Error::Keys);
    }
    delete_named(api, token, name)
}

/// True while a token with the device's name exists.
pub fn r2_device_key_exists(api: &str, token: &str, name: &str) -> Result<bool, Error> {
    Ok(!token_ids_named(api, token, name)?.is_empty())
}

fn permission_group(api: &str, token: &str) -> Result<String, Error> {
    let groups = call(
        api,
        token,
        "GET",
        "/client/v4/user/tokens/permission_groups",
        None,
    )?;
    groups
        .as_array()
        .into_iter()
        .flatten()
        .find(|group| group.get("name").and_then(Value::as_str) == Some(WRITE_GROUP))
        .and_then(|group| group.get("id"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or(Error::Keys)
}

fn token_ids_named(api: &str, token: &str, name: &str) -> Result<Vec<String>, Error> {
    let mut ids = Vec::new();
    let mut page = 1;
    loop {
        let listed = call(
            api,
            token,
            "GET",
            &format!("/client/v4/user/tokens?per_page=50&page={page}"),
            None,
        )?;
        let tokens = listed.as_array().cloned().unwrap_or_default();
        for found in &tokens {
            if found.get("name").and_then(Value::as_str) == Some(name)
                && let Some(id) = found.get("id").and_then(Value::as_str)
            {
                ids.push(id.to_string());
            }
        }
        if tokens.len() < 50 || page >= 100 {
            return Ok(ids);
        }
        page += 1;
    }
}

fn delete_named(api: &str, token: &str, name: &str) -> Result<(), Error> {
    for id in token_ids_named(api, token, name)? {
        call(
            api,
            token,
            "DELETE",
            &format!("/client/v4/user/tokens/{id}"),
            None,
        )?;
    }
    Ok(())
}

/// One Cloudflare API call; answers the envelope's `result`.
fn call(
    api: &str,
    token: &str,
    method: &str,
    path: &str,
    body: Option<&Value>,
) -> Result<Value, Error> {
    if token.is_empty() {
        return Err(Error::KeysDenied);
    }
    let origin = parse_origin(api.trim_end_matches('/')).map_err(|_| Error::Keys)?;
    let host = host_header(&origin);
    let payload = match body {
        Some(value) => serde_json::to_string(value).map_err(|_| Error::Keys)?,
        None => String::new(),
    };
    let message = format!(
        "{method} {path} HTTP/1.1\r\nHost: {host}\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
        payload.len()
    );
    let bytes = exchange(&origin, message.as_bytes()).map_err(|_| Error::Keys)?;
    let response = parse_response(&bytes, false).map_err(|_| Error::Keys)?;
    match response.status {
        200..=299 => {}
        401 | 403 => return Err(Error::KeysDenied),
        _ => return Err(Error::Keys),
    }
    let envelope: Value = serde_json::from_str(&response.body).map_err(|_| Error::Keys)?;
    if envelope.get("success").and_then(Value::as_bool) != Some(true) {
        return Err(Error::Keys);
    }
    Ok(envelope.get("result").cloned().unwrap_or(Value::Null))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};
    use std::thread;

    use super::*;

    #[test]
    fn the_account_and_jurisdiction_come_from_the_endpoint() {
        assert_eq!(
            r2_account("https://abc123.r2.cloudflarestorage.com"),
            Some(("abc123".to_string(), "default".to_string()))
        );
        assert_eq!(
            r2_account("https://abc123.eu.r2.cloudflarestorage.com/"),
            Some(("abc123".to_string(), "eu".to_string()))
        );
        assert_eq!(r2_account("http://127.0.0.1:4566"), None);
    }

    /// A stand-in for the Cloudflare token API: it keeps tokens by id.
    fn fake_cloudflare() -> (String, Arc<Mutex<BTreeMap<String, Value>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let base = format!("http://{}", listener.local_addr().expect("addr"));
        let tokens: Arc<Mutex<BTreeMap<String, Value>>> = Arc::default();
        let served = Arc::clone(&tokens);
        thread::spawn(move || {
            let mut counter = 0;
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let mut raw = Vec::new();
                let mut chunk = [0_u8; 4096];
                loop {
                    let read = stream.read(&mut chunk).unwrap_or(0);
                    raw.extend_from_slice(&chunk[..read]);
                    let text = String::from_utf8_lossy(&raw).to_string();
                    if let Some(end) = text.find("\r\n\r\n") {
                        let length = text
                            .lines()
                            .find_map(|line| line.strip_prefix("Content-Length: "))
                            .and_then(|value| value.trim().parse::<usize>().ok())
                            .unwrap_or(0);
                        if raw.len() >= end + 4 + length {
                            break;
                        }
                    }
                    if read == 0 {
                        break;
                    }
                }
                let text = String::from_utf8_lossy(&raw).to_string();
                let mut words = text.split(' ');
                let method = words.next().unwrap_or("").to_string();
                let path = words.next().unwrap_or("").to_string();
                let authorized = text.contains("Authorization: Bearer cf-admin");
                let body: Value = text
                    .split_once("\r\n\r\n")
                    .and_then(|(_, body)| serde_json::from_str(body).ok())
                    .unwrap_or(Value::Null);
                let mut tokens = served.lock().expect("tokens");
                let result = if !authorized {
                    None
                } else if path == "/client/v4/user/tokens/permission_groups" {
                    Some(json!([
                        {"id": "read-group", "name": "Workers R2 Storage Bucket Item Read"},
                        {"id": "write-group", "name": WRITE_GROUP},
                    ]))
                } else if method == "POST" && path == "/client/v4/user/tokens" {
                    counter += 1;
                    let id = format!("tok{counter}");
                    tokens.insert(id.clone(), body.clone());
                    Some(json!({"id": id, "value": format!("value-{counter}")}))
                } else if method == "GET" && path.starts_with("/client/v4/user/tokens?") {
                    Some(Value::Array(
                        tokens
                            .iter()
                            .map(|(id, made)| json!({"id": id, "name": made["name"]}))
                            .collect(),
                    ))
                } else if method == "DELETE" {
                    let id = path.rsplit('/').next().unwrap_or("").to_string();
                    tokens.remove(&id);
                    Some(json!({"id": id}))
                } else {
                    None
                };
                drop(tokens);
                let (status, payload) = match result {
                    Some(result) => (
                        "200 OK",
                        json!({"success": true, "result": result}).to_string(),
                    ),
                    None => ("403 Forbidden", json!({"success": false}).to_string()),
                };
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
                    payload.len()
                );
            }
        });
        (base, tokens)
    }

    #[test]
    fn each_device_gets_a_token_scoped_to_the_bucket_and_removal_deletes_only_it() {
        let (api, tokens) = fake_cloudflare();
        let endpoint = "https://abc123.r2.cloudflarestorage.com";
        let phone = "fedcba9876543210fedcba9876543210";
        let laptop = "0123456789abcdef0123456789abcdef";
        let a =
            issue_r2_device_key(&api, "cf-admin", endpoint, "bucket", "repo", phone).expect("a");
        let b =
            issue_r2_device_key(&api, "cf-admin", endpoint, "bucket", "repo", laptop).expect("b");
        assert_eq!(a.access_key_id, "tok1");
        assert_eq!(a.secret_access_key, hex::encode(Sha256::digest(b"value-1")));
        {
            let tokens = tokens.lock().expect("tokens");
            let made = tokens.get("tok1").expect("made");
            let policy = &made["policies"][0];
            assert_eq!(policy["permission_groups"][0]["id"], "write-group");
            assert_eq!(
                policy["resources"]["com.cloudflare.edge.r2.bucket.abc123_default_bucket"],
                "*"
            );
        }
        revoke_r2_device_key(&api, "cf-admin", &a.user_name).expect("revoke");
        assert!(!r2_device_key_exists(&api, "cf-admin", &a.user_name).expect("gone"));
        assert!(r2_device_key_exists(&api, "cf-admin", &b.user_name).expect("kept"));
        let again = issue_r2_device_key(&api, "cf-admin", endpoint, "bucket", "repo", laptop)
            .expect("again");
        assert_ne!(again.access_key_id, b.access_key_id);
        assert_eq!(tokens.lock().expect("tokens").len(), 1);
        assert!(matches!(
            issue_r2_device_key(&api, "wrong", endpoint, "bucket", "repo", phone),
            Err(Error::KeysDenied)
        ));
    }
}
