//! The page that creates a repository or joins one.
//!
//! A tray menu has no text fields, so the menu opens this page. It listens on
//! `127.0.0.1` only, answers only to its own host name, and every path starts
//! with a random token. The recovery password appears once, in the answer to
//! the create form, and is not written anywhere by this module.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use rand::Rng;
use satchel_core::{SetupCheck, StorageKind, StorageSetup};

use crate::secrets::SecretStore;
use crate::{AccessPair, Desktop, Error, JoinTarget, request_grant};

const BODY_LIMIT: usize = 16 * 1024;
const JOIN_TIMEOUT: Duration = Duration::from_secs(120);

/// The running page. Dropping it does not stop the listener; the process ends it.
pub struct SetupPage {
    url: String,
}

impl SetupPage {
    /// Address to open in a browser, token included.
    pub fn url(&self) -> &str {
        &self.url
    }
}

/// Start listening on a free loopback port.
pub fn start<S>(desktop: Arc<Mutex<Desktop>>, secrets: Arc<S>) -> std::io::Result<SetupPage>
where
    S: SecretStore + Send + Sync + 'static,
{
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    let port = listener.local_addr()?.port();
    let token = new_token();
    let host = format!("127.0.0.1:{port}");
    let url = format!("http://{host}/{token}/");
    let shared = Arc::new(Shared {
        desktop,
        secrets,
        token,
        host,
    });
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else {
                continue;
            };
            let shared = Arc::clone(&shared);
            thread::spawn(move || {
                let _ = serve(stream, &*shared);
            });
        }
    });
    Ok(SetupPage { url })
}

struct Shared<S> {
    desktop: Arc<Mutex<Desktop>>,
    secrets: Arc<S>,
    token: String,
    host: String,
}

struct Request {
    method: String,
    path: String,
    host: String,
    body: String,
}

fn serve<S: SecretStore>(mut stream: TcpStream, shared: &Shared<S>) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    let Some(request) = read_request(&mut stream)? else {
        return respond(&mut stream, 400, "text/plain", "bad request");
    };
    if request.host != shared.host {
        return respond(&mut stream, 403, "text/plain", "forbidden");
    }
    let prefix = format!("/{}/", shared.token);
    let Some(route) = request.path.strip_prefix(&prefix) else {
        return respond(&mut stream, 404, "text/plain", "not found");
    };
    let page = match (request.method.as_str(), route) {
        ("GET", "") => form_page(shared, None),
        ("POST", "create") => create(shared, &parse_form(&request.body)),
        ("POST", "join") => join(shared, &parse_form(&request.body)),
        _ => return respond(&mut stream, 404, "text/plain", "not found"),
    };
    respond(&mut stream, 200, "text/html; charset=utf-8", &page)
}

fn create<S: SecretStore>(shared: &Shared<S>, form: &BTreeMap<String, String>) -> String {
    let result = create_from_form(shared, form);
    match result {
        Ok(text) => text,
        Err(message) => form_page(shared, Some(&message)),
    }
}

fn create_from_form<S: SecretStore>(
    shared: &Shared<S>,
    form: &BTreeMap<String, String>,
) -> Result<String, String> {
    let kind = match field(form, "kind") {
        "folder" => StorageKind::Directory,
        "s3" => StorageKind::S3,
        "r2" => StorageKind::R2,
        "b2" => StorageKind::B2,
        _ => return Err("Choose where the repository lives.".to_string()),
    };
    let folder = required_path(form, "folder")?;
    let mut setup = StorageSetup::directory(PathBuf::new());
    setup.kind = kind;
    let device_access = if kind == StorageKind::Directory {
        setup.directory = required_path(form, "repository")?;
        None
    } else {
        setup.endpoint = field(form, "endpoint").to_string();
        setup.bucket = field(form, "bucket").to_string();
        setup.region = field(form, "region").to_string();
        setup.prefix = field(form, "prefix").to_string();
        setup.access_key_id = field(form, "access_key_id").to_string();
        setup.secret_access_key = field(form, "secret_access_key").to_string();
        Some(AccessPair {
            access_key_id: field(form, "device_access_key_id").to_string(),
            secret_access_key: field(form, "device_secret_access_key").to_string(),
        })
    };
    let mut desktop = lock(&shared.desktop);
    let created = desktop
        .create_repository(&*shared.secrets, folder, setup, device_access)
        .map_err(|err| describe(&err))?;
    let warning = match created.protection {
        SetupCheck::Ready => "Versioning is on and no lifecycle rule deletes objects.",
        SetupCheck::Unprotected => {
            "This storage has no object versioning. An overwritten file cannot be brought back."
        }
    };
    Ok(page(&format!(
        "<h1>Repository created</h1><p>{}</p>\
         <p>Save this recovery password in a password manager that opens without this \
         repository. It is shown once and the official <code>restic</code> uses it.</p>\
         <pre>{}</pre>",
        escape(warning),
        escape(&created.recovery_password)
    )))
}

fn join<S: SecretStore>(shared: &Shared<S>, form: &BTreeMap<String, String>) -> String {
    match join_from_form(shared, form) {
        Ok(text) => text,
        Err(message) => form_page(shared, Some(&message)),
    }
}

fn join_from_form<S: SecretStore>(
    shared: &Shared<S>,
    form: &BTreeMap<String, String>,
) -> Result<String, String> {
    let folder = required_path(form, "join_folder")?;
    let name = match field(form, "device_name") {
        "" => "Desktop",
        other => other,
    };
    let os = std::env::consts::OS;
    let invite = field(form, "invite");
    let target = if invite.is_empty() {
        let address = field(form, "address")
            .parse()
            .map_err(|_| "The address must look like 100.64.0.2:4000.".to_string())?;
        JoinTarget {
            address,
            code: field(form, "code").to_string(),
            fingerprint: field(form, "fingerprint").to_string(),
            device_name: name.to_string(),
            os: os.to_string(),
        }
    } else {
        JoinTarget::from_invite(invite, name, os).map_err(|err| describe(&err))?
    };
    {
        let desktop = lock(&shared.desktop);
        if desktop.profile().is_some() {
            return Err(describe(&Error::ProfileExists));
        }
    }
    // Waiting for approval must not hold the lock the tray menu needs.
    let granted = request_grant(target, JOIN_TIMEOUT).map_err(|err| describe(&err))?;
    let mut desktop = lock(&shared.desktop);
    desktop
        .join_with_grant(&*shared.secrets, folder, granted)
        .map_err(|err| describe(&err))?;
    Ok(page(
        "<h1>Joined</h1><p>This device has its own key. It syncs on its own now. \
         The recovery password stays with the device that created the repository.</p>",
    ))
}

fn describe(err: &Error) -> String {
    err.to_string()
}

fn field<'a>(form: &'a BTreeMap<String, String>, name: &str) -> &'a str {
    form.get(name).map(String::as_str).unwrap_or("").trim()
}

fn required_path(form: &BTreeMap<String, String>, name: &str) -> Result<PathBuf, String> {
    let value = field(form, name);
    if value.is_empty() || !value.starts_with('/') {
        return Err("Folders must be absolute paths.".to_string());
    }
    Ok(PathBuf::from(value))
}

fn form_page<S: SecretStore>(shared: &Shared<S>, error: Option<&str>) -> String {
    let desktop = lock(&shared.desktop);
    if desktop.profile().is_some() && error.is_none() {
        return page("<h1>Satchel</h1><p>A folder is already set up on this device.</p>");
    }
    let notice = match error {
        Some(message) => format!("<p class=\"error\">{}</p>", escape(message)),
        None => String::new(),
    };
    let base = format!("/{}/", shared.token);
    page(&format!(
        "<h1>Satchel</h1>{notice}
<h2>Create a repository</h2>
<form method=\"post\" action=\"{base}create\" onsubmit=\"busy(this,'Checking the storage…')\">
<label>Synced folder <input name=\"folder\" placeholder=\"/Users/you/Satchel\"></label>
<label>Storage <select name=\"kind\">
<option value=\"folder\">Local folder</option><option value=\"s3\">S3</option>
<option value=\"r2\">Cloudflare R2</option><option value=\"b2\">Backblaze B2</option></select></label>
<div id=\"fol\"><label>Repository folder <input name=\"repository\"></label>
<p class=\"hint\">A local folder has no object versioning: an overwritten file cannot be brought back.</p></div>
<div id=\"obj\" hidden><p class=\"hint\">The bucket needs versioning on and no lifecycle rule that deletes objects. \
The device key lists, reads, writes and deletes only its own locks; it must differ from the maintenance key.</p>
<label>Endpoint <input name=\"endpoint\" placeholder=\"https://s3.us-east-1.amazonaws.com\"></label>
<label>Bucket <input name=\"bucket\"></label>
<label>Region <input name=\"region\" placeholder=\"us-east-1\"></label>
<label>Prefix (may be empty) <input name=\"prefix\"></label>
<label>Maintenance access key <input name=\"access_key_id\" autocomplete=\"off\"></label>
<label>Maintenance secret <input name=\"secret_access_key\" type=\"password\" autocomplete=\"off\"></label>
<label>Device access key (list, read, write, own locks) <input name=\"device_access_key_id\" autocomplete=\"off\"></label>
<label>Device secret <input name=\"device_secret_access_key\" type=\"password\" autocomplete=\"off\"></label></div>
<button>Create</button></form>
<h2>Join with a code</h2>
<form method=\"post\" action=\"{base}join\" onsubmit=\"busy(this,'Waiting for the other device to approve…')\">
<label>Synced folder <input name=\"join_folder\" placeholder=\"/Users/you/Satchel\"></label>
<label>Device name <input name=\"device_name\" placeholder=\"Desktop\"></label>
<label>Address <input name=\"address\" placeholder=\"100.64.0.2:4000\"></label>
<label>Code <input name=\"code\" inputmode=\"numeric\"></label>
<label>Fingerprint <input name=\"fingerprint\"></label>
<label>Or paste the invite <textarea name=\"invite\" rows=\"3\"></textarea></label>
<button>Join</button></form>
<script>
const kind=document.querySelector('select[name=kind]');
function show(){{const o=kind.value!=='folder';document.getElementById('obj').hidden=!o;document.getElementById('fol').hidden=o;}}
kind.addEventListener('change',show);show();
function busy(f,t){{const b=f.querySelector('button');b.disabled=true;b.textContent=t;}}
</script>"
    ))
}

fn page(body: &str) -> String {
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>Satchel</title>\
<style>body{{font:16px system-ui;max-width:34rem;margin:2rem auto;padding:0 1rem}}\
label{{display:block;margin:.6rem 0}}input,select,textarea{{display:block;width:100%;padding:.4rem;box-sizing:border-box}}\
button{{margin-top:.8rem;padding:.5rem 1rem}}pre{{padding:.8rem;background:#eee;overflow-wrap:anywhere;white-space:pre-wrap}}\
.error{{color:#a00}}.hint{{color:#555;font-size:.9rem}}</style></head><body>{body}</body></html>"
    )
}

fn read_request(stream: &mut TcpStream) -> std::io::Result<Option<Request>> {
    let mut data = Vec::new();
    let mut chunk = [0_u8; 2048];
    let header_end;
    loop {
        let read = stream.read(&mut chunk)?;
        if read == 0 {
            return Ok(None);
        }
        data.extend_from_slice(&chunk[..read]);
        if let Some(index) = find(&data, b"\r\n\r\n") {
            header_end = index;
            break;
        }
        if data.len() > BODY_LIMIT {
            return Ok(None);
        }
    }
    let head = String::from_utf8_lossy(&data[..header_end]).to_string();
    let mut lines = head.split("\r\n");
    let Some(first) = lines.next() else {
        return Ok(None);
    };
    let mut parts = first.split(' ');
    let (Some(method), Some(path)) = (parts.next(), parts.next()) else {
        return Ok(None);
    };
    let mut host = String::new();
    let mut length = 0_usize;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let name = name.trim().to_ascii_lowercase();
        let value = value.trim();
        if name == "host" {
            host = value.to_string();
        }
        if name == "content-length" {
            length = value.parse().unwrap_or(usize::MAX);
        }
    }
    if length > BODY_LIMIT {
        return Ok(None);
    }
    let mut body = data[header_end + 4..].to_vec();
    while body.len() < length {
        let read = stream.read(&mut chunk)?;
        if read == 0 {
            return Ok(None);
        }
        body.extend_from_slice(&chunk[..read]);
        if body.len() > BODY_LIMIT {
            return Ok(None);
        }
    }
    body.truncate(length);
    Ok(Some(Request {
        method: method.to_string(),
        path: path.to_string(),
        host,
        body: String::from_utf8_lossy(&body).to_string(),
    }))
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn respond(stream: &mut TcpStream, status: u16, kind: &str, body: &str) -> std::io::Result<()> {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        403 => "Forbidden",
        _ => "Not Found",
    };
    let head = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\n\
Cache-Control: no-store\r\nReferrer-Policy: no-referrer\r\nX-Frame-Options: DENY\r\n\
Connection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes())?;
    stream.write_all(body.as_bytes())?;
    stream.flush()
}

fn parse_form(body: &str) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    for pair in body.split('&') {
        let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
        map.insert(decode(name), decode(value));
    }
    map
}

fn decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte == b'+' {
            out.push(b' ');
        } else if byte == b'%'
            && index + 2 < bytes.len()
            && let (Some(high), Some(low)) = (hex(bytes[index + 1]), hex(bytes[index + 2]))
        {
            out.push(high * 16 + low);
            index += 2;
        } else {
            out.push(byte);
        }
        index += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            other => out.push(other),
        }
    }
    out
}

fn new_token() -> String {
    let mut bytes = [0_u8; 16];
    rand::rng().fill_bytes(&mut bytes);
    let mut text = String::with_capacity(32);
    for byte in bytes {
        text.push_str(&format!("{byte:02x}"));
    }
    text
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poison) => poison.into_inner(),
    }
}
