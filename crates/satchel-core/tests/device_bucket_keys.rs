//! One bucket key per device, through IAM on a local Floci.
//!
//! Floci answers the IAM API but accepts any key on S3, so this proves the
//! calls (user, limited policy, key, removal), not that S3 refuses a removed
//! key. RustFS proves the policy itself (`rustfs_policy`).

#[path = "support/floci.rs"]
mod floci;

use floci::*;
use satchel_core::{
    KeyAdmin, device_bucket_key_exists, device_key_user, issue_device_bucket_key,
    revoke_device_bucket_key,
};

const PHONE: &str = "fedcba9876543210fedcba9876543210";
const LAPTOP: &str = "0123456789abcdef0123456789abcdef";

#[test]
fn each_device_gets_its_own_key_and_removing_one_leaves_the_other() {
    let floci = LocalFloci::start();
    let admin = KeyAdmin {
        s3_endpoint: floci.endpoint(),
        access_key_id: "maintenance",
        secret_access_key: "maintenance-secret",
        cloudflare_token: "",
    };
    let phone = issue_device_bucket_key(&admin, "bucket", "repo", PHONE).expect("phone key");
    let laptop = issue_device_bucket_key(&admin, "bucket", "repo", LAPTOP).expect("laptop key");
    assert_eq!(
        phone.user_name,
        device_key_user("bucket", "repo", PHONE).expect("user")
    );
    assert_ne!(phone.access_key_id, laptop.access_key_id);
    assert!(!phone.secret_access_key.is_empty());
    assert!(device_bucket_key_exists(&admin, &phone.user_name).expect("phone listed"));
    assert!(device_bucket_key_exists(&admin, &laptop.user_name).expect("laptop listed"));
    emit_proof("iam: two devices, two users, two keys");

    revoke_device_bucket_key(&admin, &phone.user_name).expect("revoke phone");
    assert!(!device_bucket_key_exists(&admin, &phone.user_name).expect("phone gone"));
    assert!(device_bucket_key_exists(&admin, &laptop.user_name).expect("laptop kept"));
    // Removing twice is fine: the user is already gone.
    revoke_device_bucket_key(&admin, &phone.user_name).expect("revoke again");

    // Issuing again for the same device replaces its key instead of adding one.
    let again = issue_device_bucket_key(&admin, "bucket", "repo", LAPTOP).expect("reissue");
    assert_ne!(again.access_key_id, laptop.access_key_id);
    emit_proof("iam: removal leaves the other device, reissue replaces the key");
}
