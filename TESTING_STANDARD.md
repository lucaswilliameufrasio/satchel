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

`cargo test` includes the unit tests and these integration tests:

- `official_restic_restores_a_snapshot_and_the_key_file_stays_opaque` writes one snapshot and restores it with `restic`.
- `two_clients_follow_see_once_always_delete_and_conflict` runs two headless clients against one local repository.
- `recovery_password_restores_the_same_bytes_as_official_restic` restores the newest snapshot with the core and with `restic`.
- `approve_shows_the_socket_peer_and_omits_the_recovery_password`, `deny_sends_no_grant`, `wrong_code_never_reaches_approval`, and `wrong_fingerprint_never_reaches_approval` pair two local sockets. The peer address comes from the socket. A denial writes no grant. The recovery password is not a field.
- `typed_address_does_not_require_mdns` joins a typed `127.0.0.1` address with discovery off.
- `announcement_lives_only_while_the_session_does` registers a secret-free announcement only while the session is open, and only when a non-loopback local interface exists. Dropping the session closes the port. The TXT keys are checked by building the service record, not by browsing from another host.
- `one_folder_is_stored_without_secrets` keeps one folder and writes no secret into `profile.json`.
- `add_device_screen_approves_without_the_recovery_password` approves from the tray model. A stored recovery canary must not appear in the grant or the profile.
- `os_keystore_roundtrip_uses_a_throwaway_service` writes, reads, and deletes two accounts through the session secret service. The service name is `satchel-test-…`, never `satchel`.
- `tray_binary_does_not_start_without_a_display` runs the binary with `DISPLAY` and `WAYLAND_DISPLAY` removed and expects exit code 2.

`check_setup` is covered by unit tests. Those tests pass the versioning state in as a value. They do not call S3, R2, or B2. Pairing tests stay on local sockets. They do not scan a subnet and they do not open a camera.

`cargo test -p satchel-ffi --lib` builds a private repository and checks that the catalog JSON names `kept.txt` as `see` and does not contain the file bytes. `read_sample` returns those bytes afterwards.

The Android unit tests live in `android/` and do not load the native library. Run them with Gradle 8.14, `ANDROID_HOME` set to the SDK, and `JAVA_HOME` set to JDK 17 or 21. `android/local.properties` is gitignored and only holds `sdk.dir`.

```text
gradle -p android test
```

`sampleOmitsFileBytesUntilReadFile` is an instrumented test. It needs `libsatchel_ffi.so` for `x86_64-linux-android`, which is not committed. Build it with the NDK clang `x86_64-linux-android26-clang` as both `CC_x86_64_linux_android` and `CARGO_TARGET_X86_64_LINUX_ANDROID_LINKER`, then copy the release library to `android/app/src/main/jniLibs/x86_64/`. That directory is gitignored. Install the debug APK and the androidTest APK on an emulator and run `AndroidJUnitRunner`. The checked run used a dedicated AVD, `satchel_api35`, image `system-images/android-35/google_apis/x86_64`, headless.

## Not covered here

Opening a real S3 bucket. The S3 options are checked in a unit test and are not sent anywhere.
