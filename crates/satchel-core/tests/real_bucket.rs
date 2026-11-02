//! Setup against a real bucket the person owns.
//!
//! Reads `SATCHEL_TEST_*` from the environment, so run it through the secret
//! manager: `ciphera run --project <id> --env dev -- cargo test --test real_bucket -- --nocapture`.
//! Without those variables the test skips. It prints verdicts, never values.

use satchel_core::{DeviceId, Error, SetupCheck, StorageKind, StorageSetup, create_repository};

fn var(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
}

fn kind(text: &str) -> StorageKind {
    match text {
        "r2" => StorageKind::R2,
        "b2" => StorageKind::B2,
        _ => StorageKind::S3,
    }
}

#[test]
fn a_real_bucket_is_accepted_only_when_it_proves_protection() {
    let (Some(endpoint), Some(bucket), Some(key), Some(secret)) = (
        var("SATCHEL_TEST_ENDPOINT"),
        var("SATCHEL_TEST_BUCKET"),
        var("SATCHEL_TEST_ACCESS_KEY_ID"),
        var("SATCHEL_TEST_SECRET_ACCESS_KEY"),
    ) else {
        eprintln!("SKIP: SATCHEL_TEST_* is not set");
        return;
    };
    let kind_name = var("SATCHEL_TEST_KIND").unwrap_or_else(|| "s3".into());
    let base = var("SATCHEL_TEST_PREFIX").unwrap_or_default();
    let base = base.trim_matches('/');
    let unique = format!(
        "run-{}-{}",
        std::process::id(),
        DeviceId::generate().as_str()
    );
    let prefix = if base.is_empty() {
        unique
    } else {
        format!("{base}/{unique}")
    };
    let mut setup = StorageSetup::directory("unused");
    setup.kind = kind(&kind_name);
    setup.endpoint = endpoint;
    setup.bucket = bucket;
    setup.region = var("SATCHEL_TEST_REGION").unwrap_or_else(|| "us-east-1".into());
    setup.prefix = prefix;
    setup.access_key_id = key;
    setup.secret_access_key = secret;

    let device = DeviceId::generate();
    let password = satchel_core::generate_recovery_password();
    match create_repository(&setup, &device, &password) {
        Err(refusal @ (Error::ObjectProtection | Error::ProbeDenied)) => {
            let why = if matches!(refusal, Error::ProbeDenied) {
                "the key may not read bucket versioning"
            } else {
                "no versioning or a deleting lifecycle rule"
            };
            eprintln!("VERDICT {kind_name}: refused, {why}");
            let location = setup.location().expect("location");
            assert!(
                !location.config_present().expect("probe config"),
                "a refused bucket must stay empty"
            );
        }
        Ok(made) => {
            assert_eq!(made.protection, SetupCheck::Ready);
            eprintln!("VERDICT {kind_name}: accepted and created under a fresh prefix");
        }
        Err(other) => {
            let text = format!("{other:?}");
            let class = text.split(['(', ' ', '{']).next().unwrap_or("?");
            panic!("unexpected failure class {class}");
        }
    }
}
