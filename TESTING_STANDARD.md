# Testing standard

Local checks use the Rust toolchain already installed and a local directory. They do not call S3, R2, B2, or any other network service.

The integration test needs the official `restic` binary. The pinned build used here is 0.19.1 for `linux/amd64`, kept outside git at `.satchel/bin/restic`. `RESTIC_BIN` overrides that path.

## Commands

Run these from the repository root, in this order:

```text
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test
```

`cargo test` includes the unit tests and `official_restic_restores_a_snapshot_and_the_key_file_stays_opaque`, which creates a temporary repository, writes one snapshot, and restores it with `restic`.

## Not covered here

Opening a real S3 bucket. The S3 options are checked in a unit test and are not sent anywhere.
