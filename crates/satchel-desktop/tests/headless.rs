use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

#[cfg(not(target_os = "macos"))]
#[test]
fn tray_binary_does_not_start_without_a_display() {
    let binary = env!("CARGO_BIN_EXE_satchel-desktop");
    let output = Command::new(binary)
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .output()
        .expect("run");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("no display"));
    assert!(!stderr.contains("recovery"));
}

/// macOS treats a connected display as the session, not `DISPLAY`.
#[cfg(target_os = "macos")]
#[test]
fn tray_binary_follows_the_connected_display() {
    let binary = env!("CARGO_BIN_EXE_satchel-desktop");
    let root = tempfile::tempdir().expect("temp");
    let service = format!(
        "satchel-test-{}",
        root.path()
            .file_name()
            .and_then(|name| name.to_str())
            .expect("temp name")
    );
    let mut child = Command::new(binary)
        .env("XDG_CONFIG_HOME", root.path())
        .env("SATCHEL_KEYCHAIN_SERVICE", &service)
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("run");
    thread::sleep(Duration::from_millis(1200));
    let status = child.try_wait().expect("wait");
    if active_display_count() == 0 {
        let _ = child.kill();
        let output = child.wait_with_output().expect("output");
        assert_eq!(output.status.code(), Some(2));
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("no display"));
        assert!(!stderr.contains("recovery"));
        return;
    }
    assert!(status.is_none(), "tray exited while a display is connected");
    let _ = child.kill();
    let output = child.wait_with_output().expect("output");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("recovery"));
}

#[cfg(target_os = "macos")]
fn active_display_count() -> u32 {
    let mut count = 0u32;
    let status = unsafe { CGGetActiveDisplayList(0, std::ptr::null_mut(), &mut count) };
    if status == 0 { count } else { 0 }
}

#[cfg(target_os = "macos")]
#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGGetActiveDisplayList(
        max_displays: u32,
        active_displays: *mut u32,
        display_count: *mut u32,
    ) -> i32;
}
