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
    assert!(
        again.body.contains("Folders on this computer"),
        "{}",
        again.body
    );

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
    assert!(
        form_pt
            .body
            .contains("Celulares e outros computadores não alcançam")
    );

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

#[test]
fn add_device_page_shows_the_invite_and_takes_the_decision() {
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
    let page = start_setup_page_with_picker(Arc::clone(&first), Arc::clone(&first_store), None)
        .expect("page");
    let (host, base) = split_url(page.url());
    let add = format!("{base}add");

    // Before a folder is set up, the page says what to do first.
    let early = http(&host, &host, "GET", &add, "");
    assert!(
        early.body.contains("Set up this device first"),
        "{}",
        early.body
    );

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

    // The tray would open the invite on the mesh; the test keeps it on loopback.
    let offer = first
        .lock()
        .expect("lock")
        .open_add_device(ListenConfig {
            bind: "127.0.0.1:0".parse().expect("bind"),
            timeout: Duration::from_secs(90),
            advertise: false,
        })
        .expect("screen");
    let shown = http(&host, &host, "GET", &add, "");
    assert_eq!(shown.status, 200);
    assert!(shown.body.contains(&offer.code), "the code, large");
    assert!(shown.body.contains(&offer.address.to_string()));
    assert!(shown.body.contains(&offer.fingerprint[..4]));
    assert!(shown.body.contains("add/qr.png"));
    // A folder repository is named, and the phone problem is said before anyone joins.
    assert!(
        shown.body.contains("a folder on this computer"),
        "{}",
        shown.body
    );
    assert!(shown.body.contains("A phone can't reach it"));
    let mut qr = TcpStream::connect(&host).expect("connect");
    write!(
        qr,
        "GET {add}/qr.png HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n"
    )
    .expect("write");
    let mut png = Vec::new();
    qr.read_to_end(&mut png).expect("read");
    assert!(png.starts_with(b"HTTP/1.1 200"));
    assert!(png.windows(4).any(|window| window == b"\x89PNG"), "a PNG");
    let waiting = http(&host, &host, "GET", &format!("{add}/state"), "");
    assert!(waiting.body.contains("\"waiting\""), "{}", waiting.body);

    let second_store = Arc::new(MemoryStore::default());
    let second = Arc::new(Mutex::new(
        Desktop::open(root.path().join("config-second"))
            .expect("open")
            .with_language(Language::En),
    ));
    let second_page =
        start_setup_page_with_picker(Arc::clone(&second), Arc::clone(&second_store), None)
            .expect("page");
    let (second_host, second_base) = split_url(second_page.url());
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
    // The tray loop polls; here the test does, and reads what the page reads.
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        first.lock().expect("lock").poll();
        let state = http(&host, &host, "GET", &format!("{add}/state"), "");
        if state.body.contains("\"pending\"") {
            assert!(state.body.contains("\"Second\""), "{}", state.body);
            assert!(state.body.contains(&offer.code));
            break;
        }
        assert!(Instant::now() < deadline, "no approval request arrived");
        thread::sleep(Duration::from_millis(30));
    }
    let approved = http(&host, &host, "POST", &format!("{add}/approve"), "");
    assert!(approved.body.contains("\"ok\":true"), "{}", approved.body);
    let joined = joiner.join().expect("thread");
    assert!(joined.body.contains("Joined"), "{}", joined.body);
    let closed = http(&host, &host, "GET", &format!("{add}/state"), "");
    assert!(closed.body.contains("\"closed\""), "{}", closed.body);

    // A second approval has nothing to approve, and says so in words.
    let again = http(&host, &host, "POST", &format!("{add}/approve"), "");
    assert!(again.body.contains("\"ok\":false"), "{}", again.body);
    assert!(again.body.contains("\"message\""));
    let view = first.lock().expect("lock").tray_view();
    assert!(
        view.items.iter().any(|item| item.label.starts_with("⚠ ")),
        "the tray shows it too"
    );
    // After a sync, the tray names the storage that keeps no old versions.
    first
        .lock()
        .expect("lock")
        .publish(&*first_store)
        .expect("sync");
    let view = first.lock().expect("lock").tray_view();
    assert!(
        view.items
            .iter()
            .any(|item| item.label.contains("repo keeps no old versions")),
        "{:?}",
        view.items
            .iter()
            .map(|item| &item.label)
            .collect::<Vec<_>>()
    );

    // An invite nobody answers ends, and the page says why.
    first
        .lock()
        .expect("lock")
        .open_add_device(ListenConfig {
            bind: "127.0.0.1:0".parse().expect("bind"),
            timeout: Duration::from_millis(300),
            advertise: false,
        })
        .expect("screen");
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        first.lock().expect("lock").poll();
        let state = http(&host, &host, "GET", &format!("{add}/state"), "");
        if state.body.contains("\"closed\"") {
            assert!(state.body.contains("\"problem\""), "{}", state.body);
            break;
        }
        assert!(Instant::now() < deadline, "the invite never ended");
        thread::sleep(Duration::from_millis(50));
    }

    // A folder repository opens no invite on any device; a wrong address gets a page, not plain text.
    let (member_host, member_base) = split_url(second_page.url());
    let member = http(
        &member_host,
        &member_host,
        "GET",
        &format!("{member_base}add"),
        "",
    );
    assert!(member.body.contains("can&#39;t reach"), "{}", member.body);
    let gone = http(&host, &host, "GET", &format!("{add}/nothing"), "");
    assert_eq!(gone.status, 404);
    assert!(gone.body.contains("This page is gone"), "{}", gone.body);
    let stranger = http(&host, "evil.example", "GET", &add, "");
    assert_eq!(stranger.status, 403);
    assert!(stranger.body.contains("didn't open"));
}

#[test]
fn the_device_page_adds_stops_and_disconnects() {
    let root = tempdir().expect("temp");
    let docs = root.path().join("Documentos");
    let photos = root.path().join("Fotos");
    fs::create_dir_all(&docs).expect("docs");
    fs::create_dir_all(&photos).expect("photos");
    let store = Arc::new(MemoryStore::default());
    let desktop = Arc::new(Mutex::new(
        Desktop::open(root.path().join("config"))
            .expect("open")
            .with_language(Language::En),
    ));
    let page =
        start_setup_page_with_picker(Arc::clone(&desktop), Arc::clone(&store), None).expect("page");
    let (host, base) = split_url(page.url());
    let created = http(
        &host,
        &host,
        "POST",
        &format!("{base}create"),
        &form(&[
            ("kind", "folder"),
            ("folder", docs.to_str().expect("path")),
            (
                "repository",
                root.path().join("copies").to_str().expect("path"),
            ),
        ]),
    );
    assert!(created.body.contains("Repository created"));

    let shown = http(&host, &host, "GET", &base, "");
    assert!(shown.body.contains("Folders on this computer"));
    assert!(shown.body.contains("Documentos"));
    assert!(
        shown.body.contains("Add device is off"),
        "a folder repository says why"
    );

    // A folder repository opens no invite from the page.
    let add = http(&host, &host, "GET", &format!("{base}add"), "");
    assert!(add.body.contains("can&#39;t reach"), "{}", add.body);
    assert!(desktop.lock().expect("lock").offer().is_none());

    let added = http(
        &host,
        &host,
        "POST",
        &format!("{base}folders/add"),
        &form(&[
            ("path", photos.to_str().expect("path")),
            ("name", "Fotos da família"),
        ]),
    );
    assert!(
        added.body.contains("“Fotos da família” syncs from now on."),
        "{}",
        added.body
    );
    // A change that worked is a toast; the inline line stays only for a page without script.
    assert!(added.body.contains("data-toast=\"ok\""));
    assert!(added.body.contains("function toast("));
    desktop
        .lock()
        .expect("lock")
        .publish(&*store)
        .expect("sync");
    let listed = http(&host, &host, "GET", &base, "");
    assert!(listed.body.contains("<h2>Devices</h2>"), "{}", listed.body);
    assert!(listed.body.contains("class=\"badge\">main"));
    assert!(listed.body.contains("class=\"badge quiet\">this one"));
    // Leaving is confirmed inside the page, not by a browser dialog.
    assert!(!listed.body.contains("confirm("));
    assert!(listed.body.contains("id=\"leave-confirm\" hidden"));
    assert!(
        !listed.body.contains("Make this a main device"),
        "already main"
    );
    let nested = http(
        &host,
        &host,
        "POST",
        &format!("{base}folders/add"),
        &form(&[("path", docs.to_str().expect("path"))]),
    );
    assert!(
        nested.body.contains("role=\"alert\" data-toast=\"error\""),
        "{}",
        nested.body
    );
    let lane = desktop
        .lock()
        .expect("lock")
        .profile()
        .expect("profile")
        .folders[1]
        .lane
        .clone()
        .expect("id");
    let stopped = http(
        &host,
        &host,
        "POST",
        &format!("{base}folders/stop"),
        &form(&[("lane", &lane)]),
    );
    assert!(stopped.body.contains("Stopped."), "{}", stopped.body);
    assert_eq!(
        desktop
            .lock()
            .expect("lock")
            .profile()
            .expect("profile")
            .folders
            .len(),
        1
    );

    // The creator must say the recovery password is saved before leaving.
    let refused = http(&host, &host, "POST", &format!("{base}disconnect"), "");
    assert!(
        refused.body.contains("recovery password is saved"),
        "{}",
        refused.body
    );
    assert!(desktop.lock().expect("lock").profile().is_some());
    let left = http(
        &host,
        &host,
        "POST",
        &format!("{base}disconnect"),
        "saved=yes",
    );
    assert!(left.body.contains("Disconnected"), "{}", left.body);
    assert!(desktop.lock().expect("lock").profile().is_none());
    assert!(store.get(Account::RecoveryPassword).expect("get").is_none());
    assert!(
        http(&host, &host, "GET", &base, "")
            .body
            .contains("Set up this device")
    );
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
