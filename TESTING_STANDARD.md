# Testing standard

Local checks use the Rust toolchain already installed. `cargo test` runs every test, including the local Floci flow. That test starts Docker. It does not call S3, R2, or B2.

The integration test needs the official `restic` binary. The pinned build used here is 0.19.1 for `linux/amd64`, kept outside git at `.satchel/bin/restic`. `RESTIC_BIN` overrides that path.

## Commands

Run these from the repository root, in this order:

```text
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test
```

`cargo test` includes the unit tests and these integration tests:

- `official_restic_restores_a_snapshot_and_the_key_file_stays_opaque` writes one snapshot and restores it with `restic`.
- `two_clients_follow_see_once_always_delete_and_conflict` runs two headless clients against one local repository.
- `recovery_password_restores_the_same_bytes_as_official_restic` restores the newest snapshot with the core and with `restic`.
- `device_key_opens_without_the_recovery_password` adds a second key, restores with that password through the core and through `restic`, then removes the key. The recovery password still restores. The key that opened the repository is not removed.
- `approve_shows_the_socket_peer_and_omits_the_recovery_password`, `deny_sends_no_grant`, `wrong_code_never_reaches_approval`, and `wrong_fingerprint_never_reaches_approval` pair two local sockets. The peer address comes from the socket. A denial writes no grant. The recovery password is not a field.
- `typed_address_does_not_require_mdns` joins a typed `127.0.0.1` address with discovery off.
- `announcement_lives_only_while_the_session_does` registers a secret-free announcement only while the session is open, and only when a non-loopback local interface exists. Dropping the session closes the port. The TXT keys are checked by building the service record, not by browsing from another host.
- `one_folder_is_stored_without_secrets` keeps one folder and writes no secret into `profile.json`.
- `add_device_screen_approves_without_the_recovery_password` approves from the tray model. A stored recovery canary must not appear in the grant or the profile.
- `os_keystore_roundtrip_uses_a_throwaway_service` writes, reads, and deletes two accounts through the session secret service. The service name is `satchel-test-…`, never `satchel`.
- `tray_binary_does_not_start_without_a_display` runs the binary with `DISPLAY` and `WAYLAND_DISPLAY` removed and expects exit code 2.

`check_setup` is covered by unit tests. Those tests pass the versioning state in as a value. They do not call S3, R2, or B2. Pairing tests stay on local sockets. They do not scan a subnet and they do not open a camera.

`cargo test -p satchel-ffi --lib` builds a private repository and checks that the catalog JSON names `kept.txt` as `see` and does not contain the file bytes. `read_sample` returns those bytes afterwards and still does not write the work copy. `mark_sample` with `once` then writes a snapshot, the work copy, and a later catalog reads `once` without including the file bytes. An unknown follow name fails before that write.

`phone_mark_reopens_from_a_new_snapshot` publishes one file from a desktop client, then a phone client stores `once` as a new snapshot. A new phone process reads that mark, and the desktop catalog lists both device ids. Storing `see` removes the phone from that list and does not delete the copy already downloaded.

The Android unit tests live in `android/` and do not load the native library. Run them with Gradle 8.14, `ANDROID_HOME` set to the SDK, and `JAVA_HOME` set to JDK 17 or 21. `android/local.properties` is gitignored and only holds `sdk.dir`.

```text
gradle -p android test
```

`sampleOmitsFileBytesUntilReadFile` is an instrumented test. It needs `libsatchel_ffi.so` for `x86_64-linux-android`, which is not committed. Build it with the NDK clang `x86_64-linux-android26-clang` as both `CC_x86_64_linux_android` and `CARGO_TARGET_X86_64_LINUX_ANDROID_LINKER`, then copy the release library to `android/app/src/main/jniLibs/x86_64/`. That directory is gitignored. Install the debug APK and the androidTest APK on an emulator and run `AndroidJUnitRunner`. The checked run used a dedicated AVD, `satchel_api35`, image `system-images/android-35/google_apis/x86_64`, headless.

`desktop_publishes_and_the_phone_reads_metadata_only` runs as part of `cargo test`. It starts `floci/floci:2.3.0` with testcontainers on the rootless Docker context, publishes port 4566 on `127.0.0.1` only, and removes that container and its anonymous volumes when the test ends, including on failure. A leftover container whose name starts with `satchel-floci-` is removed before the new one starts. The image must already be local; a missing image fails the test and is not pulled. `FLOCI_ENDPOINT` is not read. The test creates a bucket, enables versioning, requires `GetBucketLifecycleConfiguration` to fail, publishes `notes/from-desktop.txt`, and checks that a second client lists that name as `see` without writing the work copy. Official `restic` 0.19.1 then restores the same bytes from `s3:<endpoint>/<bucket>/repo` with `-o s3.bucket-lookup=path`. The access key is `test`. `FLOCI_PHONE_SPEC`, when set, writes a mode `0600` spec and replaces `127.0.0.1` with `10.0.2.2`.

`SharedStoreTest` is an instrumented test. Pass the spec path with `-e storeSpec <path>`. It needs the `INTERNET` permission and `libsatchel_ffi.so`. The checked run on `satchel_api35` showed `notes/from-desktop.txt`, `2023-11-14T22:13:20Z`, the desktop id, and Ver. The work file stayed absent before and after Baixar agora. `readStore` returned the bytes.

## Not covered here

A real S3, R2, or B2 bucket. The tray does not publish to the bucket. The Floci clients both open with the recovery password fixture. The Android screen was not opened again after the mark started writing a snapshot. iOS is not covered.
