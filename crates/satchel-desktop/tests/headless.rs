use std::process::Command;

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
