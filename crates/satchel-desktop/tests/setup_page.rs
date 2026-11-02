//! The page the tray opens: create a repository or join one, over real HTTP.

use std::fs;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use satchel_desktop::{Account, Desktop, MemoryStore, Role, SecretStore, start_setup_page};
use satchel_pair::ListenConfig;
use tempfile::tempdir;

struct Reply {
    status: u16,
    body: String,
}

fn http(host_port: &str, host_header: &str, method: &str, path: &str, body: &str) -> Reply {
    let mut stream = TcpStream::connect(host_port).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(90)))
        .expect("timeout");
    let request = format!(
        "{method} {path} HTTP/1.1\r\nHost: {host_header}\r\nContent-Type: application/x-www-form-urlencoded\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(request.as_bytes()).expect("write");
    let mut text = String::new();
    stream.read_to_string(&mut text).expect("read");
    let status = text
        .split(' ')
        .nth(1)
        .and_then(|part| part.parse().ok())
        .expect("status");
    let body = text.split("\r\n\r\n").nth(1).unwrap_or("").to_string();
    Reply { status, body }
}

fn encode(text: &str) -> String {
    let mut out = String::new();
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_' || byte == b'.' {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn form(pairs: &[(&str, &str)]) -> String {
    let mut parts = Vec::new();
    for (name, value) in pairs {
        parts.push(format!("{name}={}", encode(value)));
    }
    parts.join("&")
}

fn split_url(url: &str) -> (String, String) {
    let rest = url.strip_prefix("http://").expect("http");
    let (host, path) = rest.split_once('/').expect("path");
    (host.to_string(), format!("/{path}"))
}

#[test]
fn page_creates_and_joins_and_refuses_strangers() {
    let root = tempdir().expect("temp");
    let first_folder = root.path().join("first");
    let second_folder = root.path().join("second");
    fs::create_dir_all(&first_folder).expect("first");
    fs::create_dir_all(&second_folder).expect("second");

    let first_store = Arc::new(MemoryStore::default());
    let first = Arc::new(Mutex::new(
        Desktop::open(root.path().join("config-first")).expect("open"),
    ));
    let page = start_setup_page(Arc::clone(&first), Arc::clone(&first_store)).expect("page");
    let (host, base) = split_url(page.url());

    // Only its own host name and its token are answered.
    assert_eq!(http(&host, &host, "GET", &base, "").status, 200);
    assert_eq!(http(&host, "evil.example:80", "GET", &base, "").status, 403);
    assert_eq!(http(&host, &host, "GET", "/not-the-token/", "").status, 404);

    // Relative path and missing fields are refused with a message, not a profile.
    let refused = http(
        &host,
        &host,
        "POST",
        &format!("{base}create"),
        &form(&[("kind", "folder"), ("folder", "relative/path")]),
    );
    assert_eq!(refused.status, 200);
    assert!(refused.body.contains("absolute"));
    assert!(first.lock().expect("lock").profile().is_none());

    // Create a folder repository.
    let repository = root.path().join("repo");
    let created = http(
        &host,
        &host,
        "POST",
        &format!("{base}create"),
        &form(&[
            ("kind", "folder"),
            ("folder", first_folder.to_str().expect("path")),
            ("repository", repository.to_str().expect("path")),
        ]),
    );
    assert!(created.body.contains("Repository created"));
    let password = first_store
        .get(Account::RecoveryPassword)
        .expect("get")
        .expect("stored");
    assert!(created.body.contains(&password), "shown once");
    assert!(created.body.contains("no object versioning"));
    assert_eq!(
        first.lock().expect("lock").profile().expect("profile").role,
        Role::Maintainer
    );
    // The form is gone once a folder exists, and the password is not repeated.
    let again = http(&host, &host, "GET", &base, "");
    assert!(!again.body.contains(&password));
    assert!(again.body.contains("already set up"));

    // A second desktop joins through its own page, typing only the code.
    let second_store = Arc::new(MemoryStore::default());
    let second = Arc::new(Mutex::new(
        Desktop::open(root.path().join("config-second")).expect("open"),
    ));
    let second_page =
        start_setup_page(Arc::clone(&second), Arc::clone(&second_store)).expect("page");
    let (second_host, second_base) = split_url(second_page.url());
    let offer = first
        .lock()
        .expect("lock")
        .open_add_device(ListenConfig {
            bind: "127.0.0.1:0".parse().expect("bind"),
            timeout: Duration::from_secs(90),
            advertise: false,
        })
        .expect("screen");
    let join_body = form(&[
        ("join_folder", second_folder.to_str().expect("path")),
        ("device_name", "Second"),
        ("address", &offer.address.to_string()),
        ("code", &offer.code),
        ("fingerprint", &offer.fingerprint),
    ]);
    let joiner = thread::spawn(move || {
        http(
            &second_host,
            &second_host,
            "POST",
            &format!("{second_base}join"),
            &join_body,
        )
    });
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        {
            let mut desktop = first.lock().expect("lock");
            desktop.poll();
            if desktop.pending().is_some() {
                assert_eq!(desktop.pending().expect("pending").device_name, "Second");
                desktop.approve_pending(&*first_store).expect("approve");
                break;
            }
        }
        assert!(Instant::now() < deadline, "no approval request arrived");
        thread::sleep(Duration::from_millis(30));
    }
    let joined = joiner.join().expect("thread");
    assert!(joined.body.contains("Joined"), "{}", joined.body);
    assert!(!joined.body.contains(&password));
    assert_eq!(
        second
            .lock()
            .expect("lock")
            .profile()
            .expect("profile")
            .role,
        Role::Member
    );
    assert!(
        second_store
            .get(Account::RecoveryPassword)
            .expect("get")
            .is_none()
    );
}
