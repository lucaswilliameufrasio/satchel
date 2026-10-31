//! Policy for the credential a phone stores after pairing.
//!
//! The phone may list, read, and write the repository. It may delete only its
//! own lock objects. Packs, indexes, snapshots, and keys stay with the desktop
//! credential. This document is what the person attaches to that key. A store
//! that has no identity policy cannot enforce it by itself.

use serde_json::json;

use crate::Error;

/// IAM policy JSON. `prefix` is the repository root inside the bucket and may be empty.
pub fn phone_credential_policy(bucket: &str, prefix: &str) -> Result<String, Error> {
    validate_bucket(bucket)?;
    validate_prefix(prefix)?;
    let policy = json!({
        "Version": "2012-10-17",
        "Statement": [
            {
                "Sid": "List",
                "Effect": "Allow",
                "Action": ["s3:GetBucketLocation", "s3:GetBucketVersioning", "s3:ListBucket"],
                "Resource": format!("arn:aws:s3:::{bucket}"),
            },
            {
                "Sid": "ReadWrite",
                "Effect": "Allow",
                "Action": ["s3:GetObject", "s3:PutObject"],
                "Resource": object_arn(bucket, prefix, "*"),
            },
            {
                "Sid": "DeleteOwnLocks",
                "Effect": "Allow",
                "Action": "s3:DeleteObject",
                "Resource": object_arn(bucket, prefix, "locks/*"),
            },
            {
                "Sid": "KeepHistory",
                "Effect": "Deny",
                "Action": [
                    "s3:DeleteBucket",
                    "s3:DeleteObject",
                    "s3:DeleteObjectVersion",
                    "s3:PutBucketVersioning",
                    "s3:PutLifecycleConfiguration",
                ],
                "Resource": [
                    object_arn(bucket, prefix, "config"),
                    object_arn(bucket, prefix, "data/*"),
                    object_arn(bucket, prefix, "index/*"),
                    object_arn(bucket, prefix, "keys/*"),
                    object_arn(bucket, prefix, "snapshots/*"),
                ],
            },
        ],
    });
    serde_json::to_string(&policy).map_err(|_| Error::EmptyField { field: "policy" })
}

/// True only for a restic lock object under this repository prefix.
///
/// The lock id is 64 lowercase hex characters. Packs, indexes, snapshots, and
/// keys are never deletable with the phone credential.
pub fn phone_may_delete(prefix: &str, object_key: &str) -> bool {
    if validate_prefix(prefix).is_err() {
        return false;
    }
    let locks = if prefix.is_empty() {
        "locks/".to_string()
    } else {
        format!("{prefix}/locks/")
    };
    let Some(id) = object_key.strip_prefix(&locks) else {
        return false;
    };
    if id.len() != 64 {
        return false;
    }
    for character in id.chars() {
        if !character.is_ascii_hexdigit() || character.is_ascii_uppercase() {
            return false;
        }
    }
    true
}

fn object_arn(bucket: &str, prefix: &str, tail: &str) -> String {
    if prefix.is_empty() {
        return format!("arn:aws:s3:::{bucket}/{tail}");
    }
    format!("arn:aws:s3:::{bucket}/{prefix}/{tail}")
}

fn validate_bucket(bucket: &str) -> Result<(), Error> {
    if bucket.is_empty() || bucket.len() > 63 || bucket.contains('/') || bucket.contains('"') {
        return Err(Error::EmptyField { field: "bucket" });
    }
    for character in bucket.chars() {
        if character.is_control() {
            return Err(Error::EmptyField { field: "bucket" });
        }
    }
    Ok(())
}

fn validate_prefix(prefix: &str) -> Result<(), Error> {
    if prefix.starts_with('/')
        || prefix.ends_with('/')
        || prefix.contains('"')
        || prefix.contains("..")
    {
        return Err(Error::EmptyField { field: "prefix" });
    }
    for character in prefix.chars() {
        if character.is_control() {
            return Err(Error::EmptyField { field: "prefix" });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::*;

    #[test]
    fn policy_allows_lock_deletes_and_denies_history() {
        let text = phone_credential_policy("bucket-a", "repo").expect("policy");
        let value: Value = serde_json::from_str(&text).expect("json");
        let statements = value["Statement"].as_array().expect("statements");
        let mut saw_lock_allow = false;
        let mut saw_history_deny = false;
        for statement in statements {
            let sid = statement["Sid"].as_str().unwrap_or("");
            if sid == "DeleteOwnLocks" {
                saw_lock_allow = true;
                assert_eq!(statement["Effect"].as_str(), Some("Allow"));
                assert_eq!(statement["Action"].as_str(), Some("s3:DeleteObject"));
                assert_eq!(
                    statement["Resource"].as_str(),
                    Some("arn:aws:s3:::bucket-a/repo/locks/*")
                );
            }
            if sid == "KeepHistory" {
                saw_history_deny = true;
                assert_eq!(statement["Effect"].as_str(), Some("Deny"));
                let resources = statement["Resource"].as_array().expect("resources");
                let mut saw_keys = false;
                for resource in resources {
                    if resource.as_str() == Some("arn:aws:s3:::bucket-a/repo/keys/*") {
                        saw_keys = true;
                    }
                }
                assert!(saw_keys);
            }
        }
        assert!(saw_lock_allow);
        assert!(saw_history_deny);
        assert!(!text.contains("secret"));
    }

    #[test]
    fn phone_delete_is_only_its_lock_object() {
        let lock = "repo/locks/0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        assert!(phone_may_delete("repo", lock));
        assert!(!phone_may_delete("repo", "repo/data/pack"));
        assert!(!phone_may_delete("repo", "repo/keys/abcd"));
        assert!(!phone_may_delete("repo", "repo/index/abcd"));
        assert!(!phone_may_delete("repo", "repo/snapshots/abcd"));
        assert!(!phone_may_delete("repo", "repo/locks/short"));
        assert!(!phone_may_delete(
            "repo",
            "other/locks/0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
        ));
    }
}
