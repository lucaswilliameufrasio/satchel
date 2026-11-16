//! One bucket key per device on Backblaze B2, through its native API.
//!
//! B2 has no IAM. Its application keys can be limited to one bucket and one name
//! prefix, and the maintenance key (with `writeKeys`) can create and delete them.
//! Each device gets a key named after the repository and the device id, limited
//! to the repository's prefix, so removing the device deletes only its key.
//! B2 can't limit deletion to `locks/`: inside the prefix the device key can
//! delete what it can read. Failure text does not include keys.

use serde_json::{Value, json};

use crate::Error;
use crate::iam::{DeviceBucketKey, KeyAdmin};
use crate::probe::{exchange, host_header, parse_origin, parse_response};

/// Issue, check or remove a device's own bucket key on the storage that has a key API:
/// IAM on S3, application keys on B2. Other kinds answer `None`.
pub fn own_key_issue(
    kind: crate::StorageKind,
    admin: &KeyAdmin<'_>,
    bucket: &str,
    prefix: &str,
    device_id: &str,
) -> Option<Result<DeviceBucketKey, Error>> {
    match kind {
        crate::StorageKind::S3 => Some(crate::issue_device_bucket_key(
            admin, bucket, prefix, device_id,
        )),
        crate::StorageKind::B2 if on_backblaze(admin) => Some(issue_b2_device_key(
            B2_API, admin, bucket, prefix, device_id,
        )),
        _ => None,
    }
}

/// See [`own_key_issue`].
pub fn own_key_revoke(
    kind: crate::StorageKind,
    admin: &KeyAdmin<'_>,
    name: &str,
) -> Option<Result<(), Error>> {
    match kind {
        crate::StorageKind::S3 => Some(crate::revoke_device_bucket_key(admin, name)),
        crate::StorageKind::B2 if on_backblaze(admin) => {
            Some(revoke_b2_device_key(B2_API, admin, name))
        }
        _ => None,
    }
}

/// See [`own_key_issue`].
pub fn own_key_exists(
    kind: crate::StorageKind,
    admin: &KeyAdmin<'_>,
    name: &str,
) -> Option<Result<bool, Error>> {
    match kind {
        crate::StorageKind::S3 => Some(crate::device_bucket_key_exists(admin, name)),
        crate::StorageKind::B2 if on_backblaze(admin) => {
            Some(b2_device_key_exists(B2_API, admin, name))
        }
        _ => None,
    }
}

/// The B2 key API exists only at Backblaze. A B2 profile pointed elsewhere (an
/// emulator in tests) keeps the shared key and never calls Backblaze.
fn on_backblaze(admin: &KeyAdmin<'_>) -> bool {
    let host = admin
        .s3_endpoint
        .split("://")
        .nth(1)
        .unwrap_or(admin.s3_endpoint)
        .split(['/', ':'])
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    host.ends_with(".backblazeb2.com")
}

/// Where B2 answers `b2_authorize_account`. Tests point it elsewhere.
pub const B2_API: &str = "https://api.backblazeb2.com";

/// restic deletes its own lock files, so the key needs `deleteFiles`; B2 can't limit
/// that to `locks/`.
const CAPABILITIES: [&str; 5] = [
    "listBuckets",
    "listFiles",
    "readFiles",
    "writeFiles",
    "deleteFiles",
];

/// A logged-in maintenance key.
struct Session {
    api_url: String,
    token: String,
    account_id: String,
}

/// The B2 key name for a device of a repository; the same name as the IAM user.
pub fn b2_key_name(bucket: &str, prefix: &str, device_id: &str) -> Result<String, Error> {
    crate::device_key_user(bucket, prefix, device_id)
}

/// Create the device's application key, limited to the bucket and the repository prefix.
///
/// `admin.s3_endpoint` is ignored; `api` is where B2 authorizes, normally [`B2_API`].
pub fn issue_b2_device_key(
    api: &str,
    admin: &KeyAdmin<'_>,
    bucket: &str,
    prefix: &str,
    device_id: &str,
) -> Result<DeviceBucketKey, Error> {
    let name = b2_key_name(bucket, prefix, device_id)?;
    let session = authorize(api, admin)?;
    // A key left by an earlier attempt is replaced, not kept beside the new one.
    delete_named(&session, &name)?;
    let bucket_id = bucket_id(&session, bucket)?;
    let mut body = json!({
        "accountId": session.account_id,
        "capabilities": CAPABILITIES,
        "keyName": name,
        "bucketId": bucket_id,
    });
    if !prefix.is_empty() {
        body["namePrefix"] = Value::String(format!("{}/", prefix.trim_end_matches('/')));
    }
    let created = post(&session, "b2_create_key", &body)?;
    let text = |field: &str| {
        created
            .get(field)
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or(Error::Keys)
    };
    Ok(DeviceBucketKey {
        user_name: name,
        access_key_id: text("applicationKeyId")?,
        secret_access_key: text("applicationKey")?,
    })
}

/// Delete the device's application key. A key that is already gone counts as removed.
pub fn revoke_b2_device_key(api: &str, admin: &KeyAdmin<'_>, name: &str) -> Result<(), Error> {
    if !name.starts_with("satchel-") {
        return Err(Error::Keys);
    }
    let session = authorize(api, admin)?;
    delete_named(&session, name)
}

/// True while a key with the device's name exists.
pub fn b2_device_key_exists(api: &str, admin: &KeyAdmin<'_>, name: &str) -> Result<bool, Error> {
    let session = authorize(api, admin)?;
    Ok(!key_ids_named(&session, name)?.is_empty())
}

fn authorize(api: &str, admin: &KeyAdmin<'_>) -> Result<Session, Error> {
    if admin.access_key_id.is_empty() || admin.secret_access_key.is_empty() {
        return Err(Error::KeysDenied);
    }
    let basic = base64(format!("{}:{}", admin.access_key_id, admin.secret_access_key).as_bytes());
    let answer = send(
        api,
        "GET",
        "/b2api/v2/b2_authorize_account",
        &format!("Basic {basic}"),
        None,
    )?;
    let text = |field: &str| {
        answer
            .get(field)
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or(Error::Keys)
    };
    Ok(Session {
        api_url: text("apiUrl")?,
        token: text("authorizationToken")?,
        account_id: text("accountId")?,
    })
}

fn bucket_id(session: &Session, bucket: &str) -> Result<String, Error> {
    let listed = post(
        session,
        "b2_list_buckets",
        &json!({ "accountId": session.account_id, "bucketName": bucket }),
    )?;
    listed
        .get("buckets")
        .and_then(Value::as_array)
        .and_then(|buckets| buckets.first())
        .and_then(|found| found.get("bucketId"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or(Error::Keys)
}

fn key_ids_named(session: &Session, name: &str) -> Result<Vec<String>, Error> {
    let mut ids = Vec::new();
    let mut next: Option<String> = None;
    loop {
        let mut body = json!({ "accountId": session.account_id, "maxKeyCount": 1000 });
        if let Some(start) = &next {
            body["startApplicationKeyId"] = Value::String(start.clone());
        }
        let listed = post(session, "b2_list_keys", &body)?;
        for key in listed
            .get("keys")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if key.get("keyName").and_then(Value::as_str) == Some(name)
                && let Some(id) = key.get("applicationKeyId").and_then(Value::as_str)
            {
                ids.push(id.to_string());
            }
        }
        next = listed
            .get("nextApplicationKeyId")
            .and_then(Value::as_str)
            .map(str::to_string);
        if next.is_none() {
            return Ok(ids);
        }
    }
}

fn delete_named(session: &Session, name: &str) -> Result<(), Error> {
    for id in key_ids_named(session, name)? {
        post(session, "b2_delete_key", &json!({ "applicationKeyId": id }))?;
    }
    Ok(())
}

fn post(session: &Session, call: &str, body: &Value) -> Result<Value, Error> {
    send(
        &session.api_url,
        "POST",
        &format!("/b2api/v2/{call}"),
        &session.token,
        Some(body),
    )
}

fn send(
    base: &str,
    method: &str,
    path: &str,
    authorization: &str,
    body: Option<&Value>,
) -> Result<Value, Error> {
    let origin = parse_origin(base.trim_end_matches('/')).map_err(|_| Error::Keys)?;
    let host = host_header(&origin);
    let payload = match body {
        Some(value) => serde_json::to_string(value).map_err(|_| Error::Keys)?,
        None => String::new(),
    };
    let message = format!(
        "{method} {path} HTTP/1.1\r\nHost: {host}\r\nAuthorization: {authorization}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
        payload.len()
    );
    let bytes = exchange(&origin, message.as_bytes()).map_err(|_| Error::Keys)?;
    let response = parse_response(&bytes, false).map_err(|_| Error::Keys)?;
    match response.status {
        200..=299 => serde_json::from_str(&response.body).map_err(|_| Error::Keys),
        401 | 403 => Err(Error::KeysDenied),
        _ => Err(Error::Keys),
    }
}

fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let triple = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        out.push(TABLE[(triple >> 18) as usize & 63] as char);
        out.push(TABLE[(triple >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            TABLE[(triple >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[triple as usize & 63] as char
        } else {
            '='
        });
    }
    out
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
    fn only_a_backblaze_endpoint_reaches_the_b2_key_api() {
        let local = KeyAdmin {
            s3_endpoint: "http://127.0.0.1:4566",
            access_key_id: "k",
            secret_access_key: "s",
        };
        let device = "0123456789abcdef0123456789abcdef";
        assert!(own_key_issue(crate::StorageKind::B2, &local, "b", "p", device).is_none());
        assert!(own_key_issue(crate::StorageKind::R2, &local, "b", "p", device).is_none());
        let backblaze = KeyAdmin {
            s3_endpoint: "https://s3.us-west-004.backblazeb2.com",
            access_key_id: "k",
            secret_access_key: "s",
        };
        assert!(on_backblaze(&backblaze));
        assert!(!on_backblaze(&local));
    }

    #[test]
    fn basic_auth_is_standard_base64() {
        assert_eq!(base64(b"ab"), "YWI=");
        assert_eq!(base64(b"abc"), "YWJj");
        assert_eq!(base64(b"a"), "YQ==");
    }

    /// A tiny stand-in for the B2 API: it keeps keys by id and answers the five calls.
    fn fake_b2() -> (String, Arc<Mutex<BTreeMap<String, Value>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let base = format!("http://{}", listener.local_addr().expect("addr"));
        let keys: Arc<Mutex<BTreeMap<String, Value>>> = Arc::default();
        let (served, api) = (Arc::clone(&keys), base.clone());
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
                        if raw.len() >= end + 4 + length || read == 0 {
                            break;
                        }
                    }
                    if read == 0 {
                        break;
                    }
                }
                let text = String::from_utf8_lossy(&raw).to_string();
                let path = text.split(' ').nth(1).unwrap_or("").to_string();
                let body: Value = text
                    .split_once("\r\n\r\n")
                    .and_then(|(_, body)| serde_json::from_str(body).ok())
                    .unwrap_or(Value::Null);
                let mut keys = served.lock().expect("keys");
                let answer = match path.as_str() {
                    "/b2api/v2/b2_authorize_account" => {
                        if text.contains("Basic bWFpbnQ6c2VjcmV0") {
                            json!({"apiUrl": api, "authorizationToken": "token", "accountId": "acct"})
                        } else {
                            Value::Null
                        }
                    }
                    "/b2api/v2/b2_list_buckets" => {
                        json!({"buckets": [{"bucketId": "bkt-1", "bucketName": body["bucketName"]}]})
                    }
                    "/b2api/v2/b2_create_key" => {
                        counter += 1;
                        let id = format!("key-{counter}");
                        keys.insert(id.clone(), body.clone());
                        json!({"applicationKeyId": id, "applicationKey": format!("secret-{counter}")})
                    }
                    "/b2api/v2/b2_list_keys" => {
                        let listed: Vec<Value> = keys
                            .iter()
                            .map(|(id, key)| json!({"applicationKeyId": id, "keyName": key["keyName"]}))
                            .collect();
                        json!({"keys": listed})
                    }
                    "/b2api/v2/b2_delete_key" => {
                        keys.remove(body["applicationKeyId"].as_str().unwrap_or(""));
                        json!({})
                    }
                    _ => Value::Null,
                };
                drop(keys);
                let (status, payload) = if answer.is_null() {
                    ("401 Unauthorized", "{}".to_string())
                } else {
                    ("200 OK", answer.to_string())
                };
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
                    payload.len()
                );
            }
        });
        (base, keys)
    }

    #[test]
    fn each_device_gets_a_key_limited_to_the_repository_and_removal_deletes_only_it() {
        let (api, keys) = fake_b2();
        let admin = KeyAdmin {
            s3_endpoint: "",
            access_key_id: "maint",
            secret_access_key: "secret",
        };
        let phone = "fedcba9876543210fedcba9876543210";
        let laptop = "0123456789abcdef0123456789abcdef";
        let a = issue_b2_device_key(&api, &admin, "bucket", "repo", phone).expect("phone");
        let b = issue_b2_device_key(&api, &admin, "bucket", "repo", laptop).expect("laptop");
        assert_ne!(a.access_key_id, b.access_key_id);
        {
            let keys = keys.lock().expect("keys");
            let created = keys.get(&a.access_key_id).expect("created");
            assert_eq!(created["bucketId"], "bkt-1");
            assert_eq!(created["namePrefix"], "repo/");
            assert!(!created["capabilities"].to_string().contains("Keys"));
        }
        assert!(b2_device_key_exists(&api, &admin, &a.user_name).expect("listed"));
        revoke_b2_device_key(&api, &admin, &a.user_name).expect("revoke");
        assert!(!b2_device_key_exists(&api, &admin, &a.user_name).expect("gone"));
        assert!(b2_device_key_exists(&api, &admin, &b.user_name).expect("kept"));
        // Issuing again replaces the old key instead of adding one beside it.
        let again = issue_b2_device_key(&api, &admin, "bucket", "repo", laptop).expect("again");
        assert_ne!(again.access_key_id, b.access_key_id);
        assert_eq!(keys.lock().expect("keys").len(), 1);

        let wrong = KeyAdmin {
            s3_endpoint: "",
            access_key_id: "maint",
            secret_access_key: "wrong",
        };
        assert!(matches!(
            issue_b2_device_key(&api, &wrong, "bucket", "repo", phone),
            Err(Error::KeysDenied)
        ));
    }
}
