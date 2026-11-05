# third_party

## rustic_core-0.13.0

A copy of the published `rustic_core` 0.13.0 with one change, so the library compiles for iOS.

The published crate passes a `u64` device id to `mknod` and only converts it to `i32` on macOS,
OpenBSD and 32-bit Android. iOS has the same `i32` device id, so `target_os = "ios"` is added next
to `target_os = "macos"` in the five places in `src/backend/local_destination.rs`. The whole
difference from the crates.io release is `rustic_core-0.13.0-ios.patch`.

It is wired in by `[patch.crates-io]` in the workspace `Cargo.toml`. Drop the copy and the patch
entry when a published `rustic_core` release carries this change.

The files crates.io adds to a package (`.cargo-ok`, `.cargo_vcs_info.json`, `Cargo.toml.orig`) and
the crate's own `Cargo.lock` are left out; the workspace `Cargo.lock` is the one that counts.
