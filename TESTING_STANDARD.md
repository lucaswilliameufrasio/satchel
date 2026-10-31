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
- `os_keystore_roundtrip_uses_a_throwaway_service` writes, reads, and deletes the recovery, maintenance, and phone accounts through the session secret service. The service name is `satchel-test-…`, never `satchel`.
- `publish_lists_metadata_and_revoke_removes_only_the_device_key` publishes a local folder, lists metadata without the work copy, approves a device key, and removes only that key.
- `phone_grant_adds_the_bucket_and_keeps_the_desktop_key` merges bucket, region, and prefix into the phone credential. A directory grant stays an opaque string.
- `approve_without_a_phone_credential_sends_no_grant` stores a recovery password and a maintenance credential, then sends nothing.
- `tray_binary_does_not_start_without_a_display` runs the binary with `DISPLAY` and `WAYLAND_DISPLAY` removed and expects exit code 2.

`check_setup` unit tests pass the versioning state in as a value. `probe_object_protection` is what the product calls. The Floci test is the local proof of that call. Pairing tests stay on local sockets. They do not scan a subnet and they do not open a camera. The add-device menu carries a disabled QR row. `qr_round_trips_the_invite_json` decodes that PNG back to the invite.

`cargo test -p satchel-ffi --lib` builds a private repository and checks that the catalog JSON names `kept.txt` as `see` and does not contain the file bytes. `read_sample` returns those bytes afterwards and still does not write the work copy. `mark_sample` with `once` then writes a snapshot, the work copy, and a later catalog reads `once` without including the file bytes. `read_followed` reads the snapshot named by that Baixar agora pin. An unknown follow name fails before that write. `grant_opens_with_the_device_password` publishes a folder, adds a device key, and opens the grant with that password. A `recovery` field is rejected. `join_returns_the_grant_without_a_recovery_field` returns endpoint, bucket credential, and device key password.

`phone_mark_reopens_from_a_new_snapshot` publishes one file from a desktop client, then a phone client stores `once` as a new snapshot. A new phone process reads that mark, and the desktop catalog lists both device ids. Storing `see` removes the phone from that list and does not delete the copy already downloaded.

The Android unit tests live in `android/` and do not load the native library. Run them with Gradle 8.14, `ANDROID_HOME` set to the SDK, and `JAVA_HOME` set to JDK 17 or 21. `android/local.properties` is gitignored and only holds `sdk.dir`.

```text
gradle -p android test
```

`sampleOmitsFileBytesUntilReadFile` is an instrumented test. It needs `libsatchel_ffi.so` for `x86_64-linux-android`, which is not committed. Build it with the NDK clang `x86_64-linux-android26-clang` as both `CC_x86_64_linux_android` and `CARGO_TARGET_X86_64_LINUX_ANDROID_LINKER`, then copy the release library to `android/app/src/main/jniLibs/x86_64/`. That directory is gitignored. Install the debug APK and the androidTest APK on an emulator and run `AndroidJUnitRunner`. The checked run used a dedicated AVD, `satchel_api35`, image `system-images/android-35/google_apis/x86_64`, headless.

`desktop_publishes_and_the_phone_reads_metadata_only` runs as part of `cargo test`. It starts `floci/floci:2.3.0` with testcontainers on the rootless Docker context, publishes port 4566 on `127.0.0.1` only, and removes that container and its anonymous volumes when the test ends, including on failure. A leftover container whose name starts with `satchel-floci-` is removed before the new one starts. The image must already be local; a missing image fails the test and is not pulled. `FLOCI_ENDPOINT` is not read. The test creates a bucket, enables versioning, and calls `probe_object_protection`. That probe must report versioning enabled and no deleting lifecycle. `publish_folder` writes `notes/from-desktop.txt`. A second key is added. The phone opens with `device-key-password` and lists that name as `see` without writing the work copy. The key hostnames are the two opaque device ids. Official `restic` 0.19.1 then restores the same bytes from `s3:<endpoint>/<bucket>/repo` with `-o s3.bucket-lookup=path` and the recovery password. The access key is `test`. Floci has that one key, so the IAM document from `phone_credential_policy` is not enforced there. `FLOCI_PHONE_SPEC`, when set, writes a mode `0600` spec whose password is the device key and replaces `127.0.0.1` with `10.0.2.2`.

`SharedStoreTest` is an instrumented test. Pass the spec path with `-e storeSpec <path>`. It needs the `INTERNET` permission and `libsatchel_ffi.so`. The earlier checked run on `satchel_api35` showed `notes/from-desktop.txt`, `2023-11-14T22:13:20Z`, the desktop id, and Ver. That run was before Baixar agora wrote the work copy. `readStore` returned the bytes. Run it again only with a live spec from `FLOCI_PHONE_SPEC`.

`NativeCatalogTest` also covers `markOnceWritesTheWorkCopy` and `grantFileListsMetadataAndMarkOnceWritesTheWorkCopy`. On 2026-10-31 those three tests were installed on the headless AVD `satchel_api35` and run with `am instrument` for that class only. They passed. The grant file in that instrumented test opens the sample repository, which has one key. The distinct device key is `grant_opens_with_the_device_password` on the host. The same emulator then showed the sample catalog: `kept.txt`, `2023-11-14T22:13:20Z`, the publisher id, and Ver, with `cache/sample/work/kept.txt` absent. Tapping Baixar agora wrote `payload-bytes`, changed the label to Baixar agora, and listed both device ids. Starting the activity with the `satchel.pair` extra showed Endereço, Código, and Impressão digital. No live pairing was completed. JVM tests cover `assembleGrant` and `sourceFor` without loading the native library.

## Not covered here

A real S3, R2, or B2 bucket. The tray test publishes a local directory. The tray process itself was not pointed at Floci. WebDAV and SFTP stay on the warning path. Desktop YubiKey was not present. iOS, a real iPhone, and iPhone YubiKey need the Mac. OAuth, a large folder, a push relay, and remote wipe stay outside the plan.
