//! The page the tray opens: create a repository or join one, over real HTTP.

use std::fs;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use satchel_desktop::{
    Account, Desktop, FolderPicker, Language, MemoryStore, PickFor, Role, SecretStore,
    start_setup_page, start_setup_page_with_picker,
};
use satchel_pair::ListenConfig;
use tempfile::tempdir;

struct Reply {
    status: u16,
    body: String,
}

fn http(host_port: &str, host_header: &str, method: &str, path: &str, body: &str) -> Reply {
    http_with(host_port, host_header, method, path, body, "")
}

/// `extra` is more header lines, each ending in `\r\n`.
fn http_with(
    host_port: &str,
    host_header: &str,
    method: &str,
    path: &str,
    body: &str,
    extra: &str,
) -> Reply {
    let mut stream = TcpStream::connect(host_port).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(90)))
        .expect("timeout");
    let request = format!(
        "{method} {path} HTTP/1.1\r\nHost: {host_header}\r\n{extra}Content-Type: application/x-www-form-urlencoded\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
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
        Desktop::open(root.path().join("config-first"))
            .expect("open")
            .with_language(Language::En),
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
    assert!(refused.body.contains("full path"));
    assert!(first.lock().expect("lock").profile().is_none());
    assert!(
        refused.body.contains("value=\"relative/path\""),
        "what was typed comes back"
    );
    let secret_refused = http(
        &host,
        &host,
        "POST",
        &format!("{base}create"),
        &form(&[
            ("kind", "s3"),
            ("folder", "relative/path"),
            ("bucket", "my-bucket"),
            ("secret_access_key", "typed-maintenance-secret"),
            ("device_secret_access_key", "typed-device-secret"),
        ]),
    );
    assert!(secret_refused.body.contains("value=\"my-bucket\""));
    assert!(secret_refused.body.contains("value=\"s3\" selected"));
    assert!(!secret_refused.body.contains("typed-maintenance-secret"));
    assert!(!secret_refused.body.contains("typed-device-secret"));

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
        Desktop::open(root.path().join("config-second"))
            .expect("open")
            .with_language(Language::En),
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
    let (join_host, join_base) = (second_host.clone(), second_base.clone());
    let joiner = thread::spawn(move || {
        http(
            &join_host,
            &join_host,
            "POST",
            &format!("{join_base}join"),
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

    // The device that joined cannot change the recovery password and sees no button.
    let denied = http(
        &second_host,
        &second_host,
        "POST",
        &format!("{second_base}recovery"),
        "",
    );
    assert!(denied.body.contains("Not changed"), "{}", denied.body);
    let member_page = http(&second_host, &second_host, "GET", &second_base, "");
    assert!(!member_page.body.contains("Change recovery password"));

    // The creator changes it: shown once, stored, the old one no longer opens, the device keeps working.
    assert!(
        http(&host, &host, "GET", &base, "")
            .body
            .contains("Change recovery password")
    );
    let changed = http(&host, &host, "POST", &format!("{base}recovery"), "");
    assert!(
        changed.body.contains("Recovery password changed"),
        "{}",
        changed.body
    );
    let fresh = first_store
        .get(Account::RecoveryPassword)
        .expect("get")
        .expect("stored");
    assert_ne!(fresh, password);
    assert!(changed.body.contains(&fresh), "shown once");
    assert!(!changed.body.contains(&password));
    assert!(
        first_store
            .get(Account::PendingRecoveryPassword)
            .expect("get")
            .is_none()
    );
    assert!(!http(&host, &host, "GET", &base, "").body.contains(&fresh));
    // Both devices still sync: the creator with the new password, the member with its own key.
    first
        .lock()
        .expect("lock")
        .publish(&*first_store)
        .expect("creator publishes");
    second
        .lock()
        .expect("lock")
        .publish(&*second_store)
        .expect("member publishes");
}

#[test]
fn the_page_speaks_the_browser_language_and_falls_back_to_the_desktop() {
    let root = tempdir().expect("temp");
    let desktop = Arc::new(Mutex::new(
        Desktop::open(root.path().join("config"))
            .expect("open")
            .with_language(Language::En),
    ));
    let page =
        start_setup_page(Arc::clone(&desktop), Arc::new(MemoryStore::default())).expect("page");
    let (host, base) = split_url(page.url());
    let portuguese = "Accept-Language: pt-BR,pt;q=0.9,en-US;q=0.8\r\n";

    let english = http(&host, &host, "GET", &base, "");
    assert!(english.body.contains("<html lang=\"en\">"));
    assert!(english.body.contains("Set up this device"));

    let form_pt = http_with(&host, &host, "GET", &base, "", portuguese);
    assert!(form_pt.body.contains("<html lang=\"pt-BR\">"));
    assert!(form_pt.body.contains("Configurar este aparelho"));
    assert!(form_pt.body.contains("entrar com um código"));
    assert!(!form_pt.body.contains("Set up this device"));

    let refused = http_with(
        &host,
        &host,
        "POST",
        &format!("{base}create"),
        &form(&[("kind", "folder"), ("folder", "relative/path")]),
        portuguese,
    );
    assert!(
        refused.body.contains("caminho completo"),
        "{}",
        refused.body
    );
    assert!(refused.body.contains("value=\"relative/path\""));

    // A browser that asks for neither language gets the desktop's.
    desktop.lock().expect("lock").set_language(Language::PtBr);
    let fallback = http_with(&host, &host, "GET", &base, "", "Accept-Language: fr-FR\r\n");
    assert!(fallback.body.contains("Configurar este aparelho"));
}

#[test]
fn choose_fills_a_folder_from_the_system_chooser() {
    let root = tempdir().expect("temp");
    let synced = root.path().join("Satchel");
    fs::create_dir(&synced).expect("synced");
    let asked = Arc::new(Mutex::new(Vec::new()));
    let picked = synced.clone();
    let seen = Arc::clone(&asked);
    let picker: FolderPicker = Arc::new(move |language, target| {
        seen.lock().expect("lock").push((language, target));
        match target {
            PickFor::Synced => Some(picked.clone()),
            PickFor::Repository => None,
        }
    });
    let desktop = Arc::new(Mutex::new(
        Desktop::open(root.path().join("config"))
            .expect("open")
            .with_language(Language::En),
    ));
    let store = Arc::new(MemoryStore::default());
    let page = start_setup_page_with_picker(Arc::clone(&desktop), Arc::clone(&store), Some(picker))
        .expect("page");
    let (host, base) = split_url(page.url());
    let portuguese = "Accept-Language: pt-BR\r\n";

    let form_pt = http_with(&host, &host, "GET", &base, "", portuguese);
    assert_eq!(form_pt.body.matches("class=\"pick\"").count(), 3);
    assert!(form_pt.body.contains("Escolher…"));
    assert!(form_pt.body.contains("Pasta para sincronizar"));
    assert!(form_pt.body.contains("Pasta das cópias cifradas"));
    assert!(form_pt.body.contains("Celulares não alcançam"));

    let chosen = http_with(
        &host,
        &host,
        "POST",
        &format!("{base}pick"),
        "for=synced",
        portuguese,
    );
    assert_eq!(chosen.status, 200);
    assert_eq!(chosen.body, synced.to_string_lossy());
    let cancelled = http(
        &host,
        &host,
        "POST",
        &format!("{base}pick"),
        "for=repository",
    );
    assert_eq!(cancelled.status, 204);
    assert_eq!(cancelled.body, "");
    let unknown = http(
        &host,
        &host,
        "POST",
        &format!("{base}pick"),
        "for=elsewhere",
    );
    assert_eq!(unknown.status, 404);
    let stranger = http(
        &host,
        "evil.example",
        "POST",
        &format!("{base}pick"),
        "for=synced",
    );
    assert_eq!(stranger.status, 403);
    assert_eq!(
        *asked.lock().expect("lock"),
        vec![
            (Language::PtBr, PickFor::Synced),
            (Language::En, PickFor::Repository)
        ]
    );

    // The encrypted copies never go inside the folder they protect.
    let inside = http_with(
        &host,
        &host,
        "POST",
        &format!("{base}create"),
        &form(&[
            ("kind", "folder"),
            ("folder", &synced.to_string_lossy()),
            ("repository", &synced.join("copies").to_string_lossy()),
        ]),
        portuguese,
    );
    assert!(
        inside.body.contains("não pode ficar dentro"),
        "{}",
        inside.body
    );
    assert!(!synced.join("copies").exists());
    assert!(desktop.lock().expect("lock").profile().is_none());

    // Without a chooser the fields are typed, and the endpoint is not there.
    let plain = start_setup_page_with_picker(desktop, store, None).expect("page");
    let (plain_host, plain_base) = split_url(plain.url());
    let typed = http(&plain_host, &plain_host, "GET", &plain_base, "");
    assert!(!typed.body.contains("class=\"pick\""));
    assert!(typed.body.contains("id=\"folder\" name=\"folder\""));
    let missing = http(
        &plain_host,
        &plain_host,
        "POST",
        &format!("{plain_base}pick"),
        "for=synced",
    );
    assert_eq!(missing.status, 404);
}

/// Serves the page for a person or a browser tool. `SATCHEL_URL_FILE` receives the address.
/// Run with `--ignored`; it stays up for `SATCHEL_SERVE_SECS` (default 120).
#[test]
#[ignore = "serves the page for a browser; run by hand"]
fn serve_the_page_for_a_browser() {
    let root = tempdir().expect("temp");
    let desktop = Arc::new(Mutex::new(
        Desktop::open(root.path().join("config")).expect("open"),
    ));
    let page = start_setup_page(desktop, Arc::new(MemoryStore::default())).expect("page");
    if let Some(file) = std::env::var_os("SATCHEL_URL_FILE") {
        fs::write(file, page.url()).expect("url file");
    }
    let seconds = std::env::var("SATCHEL_SERVE_SECS")
        .ok()
        .and_then(|text| text.parse().ok())
        .unwrap_or(120);
    thread::sleep(Duration::from_secs(seconds));
}
