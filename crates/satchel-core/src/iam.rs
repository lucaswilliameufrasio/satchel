//! One bucket key per device, through the AWS IAM query API.
//!
//! The main device signs with the maintenance key. Each device gets an IAM user
//! named after the repository and the device id, the limited policy of
//! [`crate::phone_credential_policy`] and one access key. Removing the device
//! deletes that key, the policy and the user, so the device loses the bucket
//! too, not only the repository. Failure text does not include keys.

use std::time::{SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};

use crate::Error;
use crate::probe::{
    HttpResponse, exchange, hmac_sha256, host_header, parse_origin, parse_response, utc_stamp,
};

const POLICY_NAME: &str = "satchel-device";
const IAM_VERSION: &str = "2010-05-08";
/// IAM is global; AWS signs it in us-east-1.
const IAM_REGION: &str = "us-east-1";

/// The maintenance key, used to manage device keys.
pub struct KeyAdmin<'a> {
    /// The S3 endpoint of the repository. IAM is reached from it.
    pub s3_endpoint: &'a str,
    /// Maintenance access key id.
    pub access_key_id: &'a str,
    /// Maintenance secret.
    pub secret_access_key: &'a str,
    /// A Cloudflare API token allowed to create tokens; only R2 uses it. May be empty.
    pub cloudflare_token: &'a str,
}

/// A bucket key that belongs to one device.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceBucketKey {
    /// The IAM user that holds it. This is not a secret.
    pub user_name: String,
    /// Access key id.
    pub access_key_id: String,
    /// Secret. Goes to the device in the grant and nowhere else.
    pub secret_access_key: String,
}

/// The IAM user name for a device of a repository: `satchel-<repo>-<device id>`.
///
/// The repository part is the first 8 hex of SHA-256 over `bucket/prefix`, so two
/// repositories in one account never share a user.
pub fn device_key_user(bucket: &str, prefix: &str, device_id: &str) -> Result<String, Error> {
    if device_id.len() != 32 || !device_id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(Error::DeviceId);
    }
    let repository = hex::encode(Sha256::digest(format!("{bucket}/{prefix}").as_bytes()));
    Ok(format!("satchel-{}-{device_id}", &repository[..8]))
}

/// Where IAM answers for an S3 endpoint: AWS has one global IAM endpoint; an
/// emulator or a compatible store answers IAM on its own origin.
pub fn iam_endpoint(s3_endpoint: &str) -> String {
    let host = s3_endpoint
        .split("://")
        .nth(1)
        .unwrap_or(s3_endpoint)
        .split(['/', ':'])
        .next()
        .unwrap_or("");
    if host == "amazonaws.com" || host.ends_with(".amazonaws.com") {
        return "https://iam.amazonaws.com".to_string();
    }
    s3_endpoint.trim_end_matches('/').to_string()
}

/// Create the device's user, attach the limited policy, and make its access key.
pub fn issue_device_bucket_key(
    admin: &KeyAdmin<'_>,
    bucket: &str,
    prefix: &str,
    device_id: &str,
) -> Result<DeviceBucketKey, Error> {
    let user_name = device_key_user(bucket, prefix, device_id)?;
    let policy = crate::phone_credential_policy(bucket, prefix)?;
    let created = call(admin, &[("Action", "CreateUser"), ("UserName", &user_name)])?;
    // A user left by an earlier attempt is reused; its old keys are removed below.
    if !(ok(&created) || created.body.contains("EntityAlreadyExists")) {
        return Err(failure(&created));
    }
    let issued = (|| {
        delete_access_keys(admin, &user_name)?;
        expect_ok(call(
            admin,
            &[
                ("Action", "PutUserPolicy"),
                ("UserName", &user_name),
                ("PolicyName", POLICY_NAME),
                ("PolicyDocument", &policy),
            ],
        )?)?;
        let key = expect_ok(call(
            admin,
            &[("Action", "CreateAccessKey"), ("UserName", &user_name)],
        )?)?;
        let access_key_id = tag(&key.body, "AccessKeyId").ok_or(Error::Keys)?;
        let secret_access_key = tag(&key.body, "SecretAccessKey").ok_or(Error::Keys)?;
        Ok(DeviceBucketKey {
            user_name: user_name.clone(),
            access_key_id,
            secret_access_key,
        })
    })();
    if issued.is_err() {
        let _ = revoke_device_bucket_key(admin, &user_name);
    }
    issued
}

/// Delete the device's access keys, its policy and its user. A user that is
/// already gone counts as removed.
pub fn revoke_device_bucket_key(admin: &KeyAdmin<'_>, user_name: &str) -> Result<(), Error> {
    if !user_name.starts_with("satchel-") {
        return Err(Error::Keys);
    }
    delete_access_keys(admin, user_name)?;
    let policy = call(
        admin,
        &[
            ("Action", "DeleteUserPolicy"),
            ("UserName", user_name),
            ("PolicyName", POLICY_NAME),
        ],
    )?;
    if !(ok(&policy) || missing(&policy)) {
        return Err(failure(&policy));
    }
    let user = call(admin, &[("Action", "DeleteUser"), ("UserName", user_name)])?;
    if !(ok(&user) || missing(&user)) {
        return Err(failure(&user));
    }
    Ok(())
}

/// True while the device's user still has an access key.
pub fn device_bucket_key_exists(admin: &KeyAdmin<'_>, user_name: &str) -> Result<bool, Error> {
    let listed = call(
        admin,
        &[("Action", "ListAccessKeys"), ("UserName", user_name)],
    )?;
    if missing(&listed) {
        return Ok(false);
    }
    let listed = expect_ok(listed)?;
    Ok(!tags(&listed.body, "AccessKeyId").is_empty())
}

fn delete_access_keys(admin: &KeyAdmin<'_>, user_name: &str) -> Result<(), Error> {
    let listed = call(
        admin,
        &[("Action", "ListAccessKeys"), ("UserName", user_name)],
    )?;
    if missing(&listed) {
        return Ok(());
    }
    let listed = expect_ok(listed)?;
    for key_id in tags(&listed.body, "AccessKeyId") {
        let deleted = call(
            admin,
            &[
                ("Action", "DeleteAccessKey"),
                ("UserName", user_name),
                ("AccessKeyId", &key_id),
            ],
        )?;
        if !(ok(&deleted) || missing(&deleted)) {
            return Err(failure(&deleted));
        }
    }
    Ok(())
}

fn ok(response: &HttpResponse) -> bool {
    (200..300).contains(&response.status)
}

fn missing(response: &HttpResponse) -> bool {
    response.status == 404 || response.body.contains("NoSuchEntity")
}

fn failure(response: &HttpResponse) -> Error {
    if matches!(response.status, 401 | 403) {
        Error::KeysDenied
    } else {
        Error::Keys
    }
}

fn expect_ok(response: HttpResponse) -> Result<HttpResponse, Error> {
    if ok(&response) {
        Ok(response)
    } else {
        Err(failure(&response))
    }
}

/// One signed IAM query-API call. The parameters go form-encoded in the body.
fn call(admin: &KeyAdmin<'_>, parameters: &[(&str, &str)]) -> Result<HttpResponse, Error> {
    if admin.access_key_id.is_empty() || admin.secret_access_key.is_empty() {
        return Err(Error::KeysDenied);
    }
    let endpoint = iam_endpoint(admin.s3_endpoint);
    let origin = parse_origin(&endpoint).map_err(|_| Error::Keys)?;
    let mut body = String::new();
    for (name, value) in parameters
        .iter()
        .copied()
        .chain(std::iter::once(("Version", IAM_VERSION)))
    {
        if !body.is_empty() {
            body.push('&');
        }
        body.push_str(&form_encode(name));
        body.push('=');
        body.push_str(&form_encode(value));
    }
    let host = host_header(&origin);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| Error::Keys)?
        .as_secs();
    let (date, amz_date) = utc_stamp(now);
    let content_type = "application/x-www-form-urlencoded; charset=utf-8";
    let payload_hash = hex::encode(Sha256::digest(body.as_bytes()));
    let canonical_headers =
        format!("content-type:{content_type}\nhost:{host}\nx-amz-date:{amz_date}\n");
    let signed_headers = "content-type;host;x-amz-date";
    let canonical = format!("POST\n/\n\n{canonical_headers}\n{signed_headers}\n{payload_hash}");
    let scope = format!("{date}/{IAM_REGION}/iam/aws4_request");
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{amz_date}\n{scope}\n{}",
        hex::encode(Sha256::digest(canonical.as_bytes()))
    );
    let key = |data: &[u8], with: &[u8]| hmac_sha256(with, data).map_err(|_| Error::Keys);
    let k_date = key(
        date.as_bytes(),
        format!("AWS4{}", admin.secret_access_key).as_bytes(),
    )?;
    let k_region = key(IAM_REGION.as_bytes(), &k_date)?;
    let k_service = key(b"iam", &k_region)?;
    let k_signing = key(b"aws4_request", &k_service)?;
    let signature = hex::encode(key(string_to_sign.as_bytes(), &k_signing)?);
    let authorization = format!(
        "AWS4-HMAC-SHA256 Credential={}/{scope}, SignedHeaders={signed_headers}, Signature={signature}",
        admin.access_key_id
    );
    let message = format!(
        "POST / HTTP/1.1\r\nHost: {host}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nx-amz-date: {amz_date}\r\nAuthorization: {authorization}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let bytes = exchange(&origin, message.as_bytes()).map_err(|_| Error::Keys)?;
    parse_response(&bytes, false).map_err(|_| Error::Keys)
}

fn form_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn tag(body: &str, name: &str) -> Option<String> {
    tags(body, name).into_iter().next()
}

fn tags(body: &str, name: &str) -> Vec<String> {
    let open = format!("<{name}>");
    let close = format!("</{name}>");
    let mut found = Vec::new();
    let mut rest = body;
    while let Some(start) = rest.find(&open) {
        let after = &rest[start + open.len()..];
        let Some(end) = after.find(&close) else {
            break;
        };
        found.push(after[..end].trim().to_string());
        rest = &after[end + close.len()..];
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_user_names_the_repository_and_the_device_and_nothing_else() {
        let user = device_key_user("bucket", "repo", "0123456789abcdef0123456789abcdef").unwrap();
        assert!(user.starts_with("satchel-"));
        assert!(user.ends_with("-0123456789abcdef0123456789abcdef"));
        assert_eq!(user.len(), "satchel-".len() + 8 + 1 + 32);
        let other = device_key_user("bucket", "other", "0123456789abcdef0123456789abcdef").unwrap();
        assert_ne!(user, other);
        assert!(device_key_user("bucket", "repo", "phone").is_err());
    }

    #[test]
    fn aws_uses_the_global_iam_endpoint_and_others_their_own_origin() {
        assert_eq!(
            iam_endpoint("https://s3.us-east-1.amazonaws.com"),
            "https://iam.amazonaws.com"
        );
        assert_eq!(
            iam_endpoint("http://127.0.0.1:4566/"),
            "http://127.0.0.1:4566"
        );
    }

    #[test]
    fn every_access_key_id_in_a_listing_is_found() {
        let body = "<ListAccessKeysResult><AccessKeyMetadata><member><AccessKeyId>AKIA1</AccessKeyId></member><member><AccessKeyId> AKIA2 </AccessKeyId></member></AccessKeyMetadata></ListAccessKeysResult>";
        assert_eq!(tags(body, "AccessKeyId"), vec!["AKIA1", "AKIA2"]);
        assert_eq!(form_encode("a b/{\"x\"}"), "a%20b%2F%7B%22x%22%7D");
    }
}
