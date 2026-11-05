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

use crate::Role;
use crate::secrets::SecretStore;
use crate::text::{Language, describe};
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
    accept_language: String,
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
    if request.method == "GET" && route == "mark.png" {
        return respond_bytes(&mut stream, "image/png", MARK_PNG);
    }
    let language = Language::from_accept_language(&request.accept_language)
        .unwrap_or_else(|| lock(&shared.desktop).language());
    let page = match (request.method.as_str(), route) {
        ("GET", "") => form_page(shared, language, None, &BTreeMap::new()),
        ("POST", "create") => create(shared, language, &parse_form(&request.body)),
        ("POST", "recovery") => change_recovery(shared, language),
        ("POST", "join") => join(shared, language, &parse_form(&request.body)),
        _ => return respond(&mut stream, 404, "text/plain", "not found"),
    };
    respond(&mut stream, 200, "text/html; charset=utf-8", &page)
}

fn change_recovery<S: SecretStore>(shared: &Shared<S>, language: Language) -> String {
    let t = |en: &'static str, pt: &'static str| language.pick(en, pt);
    let mut desktop = lock(&shared.desktop);
    match desktop.change_recovery_password(&*shared.secrets) {
        Ok(password) => page(
            language,
            &format!(
                "<h1>{}</h1><p>{}</p>{}<p class=\"hint\">{}</p>",
                t("Recovery password changed", "Senha de recuperação trocada"),
                t(
                    "Save the new password in a password manager that opens without this \
                     repository. It is shown once. The old one no longer opens the repository, \
                     and every device keeps working with its own key.",
                    "Guarde a senha nova num gerenciador de senhas que abra sem este repositório. \
                     Ela aparece uma vez só. A antiga não abre mais o repositório, e cada aparelho \
                     continua funcionando com a própria chave.",
                ),
                secret_block(language, &password),
                t(
                    "Anyone who kept a copy of the old key file, or can read an older version of \
                     it from a versioned bucket, can still use the old password. This does not \
                     rotate the data key.",
                    "Quem guardou uma cópia do arquivo de chave antigo, ou consegue ler uma versão \
                     antiga dele num bucket versionado, ainda pode usar a senha antiga. Isto não \
                     troca a chave dos dados.",
                ),
            ),
        ),
        Err(err) => page(
            language,
            &format!(
                "<h1>{}</h1><p class=\"error\" role=\"alert\">{}</p>",
                t("Not changed", "Não foi trocada"),
                escape(&describe(language, &err))
            ),
        ),
    }
}

fn create<S: SecretStore>(
    shared: &Shared<S>,
    language: Language,
    form: &BTreeMap<String, String>,
) -> String {
    match create_from_form(shared, language, form) {
        Ok(text) => text,
        Err(message) => form_page(shared, language, Some(&message), form),
    }
}

fn create_from_form<S: SecretStore>(
    shared: &Shared<S>,
    language: Language,
    form: &BTreeMap<String, String>,
) -> Result<String, String> {
    let t = |en: &'static str, pt: &'static str| language.pick(en, pt);
    let kind = match field(form, "kind") {
        "folder" => StorageKind::Directory,
        "s3" => StorageKind::S3,
        "r2" => StorageKind::R2,
        "b2" => StorageKind::B2,
        _ => {
            return Err(t(
                "Choose where the repository lives.",
                "Escolha onde o repositório fica.",
            )
            .to_string());
        }
    };
    let folder = required_path(language, form, "folder")?;
    let mut setup = StorageSetup::directory(PathBuf::new());
    setup.kind = kind;
    let device_access = if kind == StorageKind::Directory {
        setup.directory = required_path(language, form, "repository")?;
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
        .map_err(|err| describe(language, &err))?;
    let warning = match created.protection {
        SetupCheck::Ready => format!(
            "<p>{}</p>",
            t(
                "Versioning is on and no lifecycle rule deletes objects.",
                "O versionamento está ligado e nenhuma regra de ciclo de vida apaga objetos.",
            )
        ),
        SetupCheck::Unprotected => format!(
            "<p class=\"warn\">{}</p>",
            t(
                "This storage has no object versioning. An overwritten file cannot be brought back.",
                "Este armazenamento não tem versionamento de objetos. Um arquivo sobrescrito não volta.",
            )
        ),
    };
    Ok(page(
        language,
        &format!(
            "<h1>{}</h1>{warning}<p>{}</p>{}<p class=\"hint\">{}</p>",
            t("Repository created", "Repositório criado"),
            t(
                "Save this recovery password in a password manager that opens without this \
                 repository. It is shown once and the official <code>restic</code> uses it.",
                "Guarde esta senha de recuperação num gerenciador de senhas que abra sem este \
                 repositório. Ela aparece uma vez só, e o <code>restic</code> oficial usa a mesma.",
            ),
            secret_block(language, &created.recovery_password),
            t(
                "Next: choose Add device in the tray menu to bring in your phone or another \
                 computer. This folder syncs on its own.",
                "Próximo passo: escolha Adicionar aparelho no menu da bandeja para trazer o \
                 celular ou outro computador. Esta pasta sincroniza sozinha.",
            ),
        ),
    ))
}

fn join<S: SecretStore>(
    shared: &Shared<S>,
    language: Language,
    form: &BTreeMap<String, String>,
) -> String {
    match join_from_form(shared, language, form) {
        Ok(text) => text,
        Err(message) => form_page(shared, language, Some(&message), form),
    }
}

fn join_from_form<S: SecretStore>(
    shared: &Shared<S>,
    language: Language,
    form: &BTreeMap<String, String>,
) -> Result<String, String> {
    let t = |en: &'static str, pt: &'static str| language.pick(en, pt);
    let folder = required_path(language, form, "join_folder")?;
    let name = match field(form, "device_name") {
        "" => "Desktop",
        other => other,
    };
    let os = std::env::consts::OS;
    let invite = field(form, "invite");
    let target = if invite.is_empty() {
        let address = field(form, "address").parse().map_err(|_| {
            t(
                "The address must look like 100.64.0.2:4000.",
                "O endereço deve ter a forma 100.64.0.2:4000.",
            )
            .to_string()
        })?;
        JoinTarget {
            address,
            code: field(form, "code").to_string(),
            fingerprint: field(form, "fingerprint").to_string(),
            device_name: name.to_string(),
            os: os.to_string(),
        }
    } else {
        JoinTarget::from_invite(invite, name, os).map_err(|err| describe(language, &err))?
    };
    {
        let desktop = lock(&shared.desktop);
        if desktop.profile().is_some() {
            return Err(describe(language, &Error::ProfileExists));
        }
    }
    // Waiting for approval must not hold the lock the tray menu needs.
    let granted = request_grant(target, JOIN_TIMEOUT).map_err(|err| describe(language, &err))?;
    let mut desktop = lock(&shared.desktop);
    desktop
        .join_with_grant(&*shared.secrets, folder, granted)
        .map_err(|err| describe(language, &err))?;
    Ok(page(
        language,
        &format!(
            "<h1>{}</h1><p>{}</p>",
            t("Joined", "Pronto, este aparelho entrou"),
            t(
                "This device has its own key. It syncs on its own now. The recovery password \
                 stays with the device that created the repository.",
                "Este aparelho tem a própria chave e já sincroniza sozinho. A senha de \
                 recuperação fica com o aparelho que criou o repositório.",
            ),
        ),
    ))
}

fn field<'a>(form: &'a BTreeMap<String, String>, name: &str) -> &'a str {
    form.get(name).map(String::as_str).unwrap_or("").trim()
}

fn required_path(
    language: Language,
    form: &BTreeMap<String, String>,
    name: &str,
) -> Result<PathBuf, String> {
    let value = field(form, name);
    if value.is_empty() || !value.starts_with('/') {
        return Err(language
            .pick(
                "Folders must be absolute paths, like /Users/you/Satchel.",
                "As pastas precisam de caminho absoluto, como /Users/voce/Satchel.",
            )
            .to_string());
    }
    Ok(PathBuf::from(value))
}

/// The create and join forms. After a refusal, `typed` refills every field but the two secrets.
fn form_page<S: SecretStore>(
    shared: &Shared<S>,
    language: Language,
    error: Option<&str>,
    typed: &BTreeMap<String, String>,
) -> String {
    let t = |en: &'static str, pt: &'static str| language.pick(en, pt);
    let value = |name: &str| match typed.get(name) {
        Some(text) if !text.is_empty() => format!(" value=\"{}\"", escape(text)),
        _ => String::new(),
    };
    let chosen = |kind: &str| {
        if typed.get("kind").map(String::as_str) == Some(kind) {
            " selected"
        } else {
            ""
        }
    };
    let invite = escape(typed.get("invite").map(String::as_str).unwrap_or(""));
    let desktop = lock(&shared.desktop);
    if let Some(profile) = desktop.profile()
        && error.is_none()
    {
        let base = format!("/{}/", shared.token);
        let change = if profile.role == Role::Maintainer {
            format!(
                "<section class=\"card\"><h2>{}</h2><p class=\"hint\">{}</p>\
                 <form method=\"post\" action=\"{base}recovery\" \
                 onsubmit=\"busy(this,'{}')\"><button>{}</button></form></section>",
                t("Recovery password", "Senha de recuperação"),
                t(
                    "Replace it if it leaked or you lost track of it. Devices keep working. \
                     You get the new one once.",
                    "Troque se ela vazou ou se você perdeu o controle dela. Os aparelhos continuam \
                     funcionando. Você vê a nova uma vez só.",
                ),
                t("Changing…", "Trocando…"),
                t("Change recovery password", "Trocar senha de recuperação"),
            )
        } else {
            String::new()
        };
        return page(
            language,
            &format!(
                "<h1>{}</h1><section class=\"card\"><p>{}</p></section>{change}",
                t("This device", "Este aparelho"),
                t(
                    "A folder is already set up on this device. It syncs on its own; use the \
                     tray menu to sync now or add a device.",
                    "Este aparelho já tem uma pasta configurada. Ela sincroniza sozinha; use o \
                     menu da bandeja para sincronizar agora ou adicionar um aparelho.",
                ),
            ),
        );
    }
    let notice = match error {
        Some(message) => format!("<p class=\"error\" role=\"alert\">{}</p>", escape(message)),
        None => String::new(),
    };
    let v_folder = value("folder");
    let v_repository = value("repository");
    let v_endpoint = value("endpoint");
    let v_bucket = value("bucket");
    let v_region = value("region");
    let v_prefix = value("prefix");
    let v_access_key_id = value("access_key_id");
    let v_device_access_key_id = value("device_access_key_id");
    let v_join_folder = value("join_folder");
    let v_device_name = value("device_name");
    let v_address = value("address");
    let v_code = value("code");
    let v_fingerprint = value("fingerprint");
    let (k_s3, k_r2, k_b2) = (chosen("s3"), chosen("r2"), chosen("b2"));
    let base = format!("/{}/", shared.token);
    let folder_example = t("/Users/you/Satchel", "/Users/voce/Satchel");
    page(
        language,
        &format!(
            "<h1>{title}</h1>
<p class=\"lead\">{lead}</p>{notice}
<section class=\"card\"><h2>{create_title}</h2>
<p class=\"hint\">{create_hint}</p>
<form method=\"post\" action=\"{base}create\" onsubmit=\"busy(this,'{checking}')\">
<label>{synced} <input name=\"folder\"{v_folder} placeholder=\"{folder_example}\"></label>
<label>{storage} <select name=\"kind\">
<option value=\"folder\">{local}</option><option value=\"s3\"{k_s3}>S3</option>
<option value=\"r2\"{k_r2}>Cloudflare R2</option><option value=\"b2\"{k_b2}>Backblaze B2</option></select></label>
<div id=\"fol\"><label>{repo_folder} <input name=\"repository\"{v_repository}></label>
<p class=\"hint\">{local_hint}</p></div>
<div id=\"obj\" hidden><p class=\"hint\">{bucket_hint}</p>
<label>Endpoint <input name=\"endpoint\"{v_endpoint} placeholder=\"https://s3.us-east-1.amazonaws.com\"></label>
<label>Bucket <input name=\"bucket\"{v_bucket}></label>
<label>{region} <input name=\"region\"{v_region} placeholder=\"us-east-1\"></label>
<label>{prefix} <input name=\"prefix\"{v_prefix}></label>
<label>{maint_key} <input name=\"access_key_id\"{v_access_key_id} autocomplete=\"off\"></label>
<label>{maint_secret} <input name=\"secret_access_key\" type=\"password\" autocomplete=\"off\"></label>
<label>{device_key} <input name=\"device_access_key_id\"{v_device_access_key_id} autocomplete=\"off\"></label>
<label>{device_secret} <input name=\"device_secret_access_key\" type=\"password\" autocomplete=\"off\"></label></div>
<button>{create}</button></form></section>
<section class=\"card\"><h2>{join_title}</h2>
<p class=\"hint\">{join_hint}</p>
<form method=\"post\" action=\"{base}join\" onsubmit=\"busy(this,'{waiting}')\">
<label>{synced} <input name=\"join_folder\"{v_join_folder} placeholder=\"{folder_example}\"></label>
<label>{device_name} <input name=\"device_name\"{v_device_name} placeholder=\"Desktop\"></label>
<label>{address} <input name=\"address\"{v_address} placeholder=\"100.64.0.2:4000\"></label>
<label>{code} <input name=\"code\"{v_code} inputmode=\"numeric\"></label>
<label>{fingerprint} <input name=\"fingerprint\"{v_fingerprint}></label>
<label>{paste} <textarea name=\"invite\" rows=\"3\">{invite}</textarea></label>
<button>{join}</button></form></section>
<script>
const kind=document.querySelector('select[name=kind]');
function show(){{const o=kind.value!=='folder';document.getElementById('obj').hidden=!o;document.getElementById('fol').hidden=o;}}
kind.addEventListener('change',show);show();
function busy(f,t){{const b=f.querySelector('button');b.disabled=true;b.textContent=t;b.setAttribute('aria-busy','true');}}
</script>",
            title = t("Set up this device", "Configurar este aparelho"),
            lead = t(
                "Create a repository here, or join one another device already holds.",
                "Crie um repositório aqui ou entre num que outro aparelho já mantém.",
            ),
            create_title = t("Create a repository", "Criar um repositório"),
            create_hint = t(
                "This device keeps the recovery password and the maintenance credential. \
                 Other devices join with a code and never type the account.",
                "Este aparelho guarda a senha de recuperação e a credencial de manutenção. \
                 Os outros entram com um código e nunca digitam a conta.",
            ),
            checking = t("Checking the storage…", "Conferindo o armazenamento…"),
            synced = t("Synced folder", "Pasta sincronizada"),
            storage = t("Storage", "Armazenamento"),
            local = t("Local folder", "Pasta local"),
            repo_folder = t("Repository folder", "Pasta do repositório"),
            local_hint = t(
                "A local folder has no object versioning: an overwritten file cannot be brought back.",
                "Uma pasta local não tem versionamento de objetos: um arquivo sobrescrito não volta.",
            ),
            bucket_hint = t(
                "The bucket needs versioning on and no lifecycle rule that deletes objects. The \
                 device key must differ from the maintenance key. It is limited to deleting its \
                 own locks only when the store applies per-key policy (AWS S3, MinIO, RustFS); \
                 on R2 and B2 it can delete everything.",
                "O bucket precisa de versionamento ligado e de nenhuma regra de ciclo de vida que \
                 apague objetos. A chave dos aparelhos tem de ser diferente da de manutenção. Ela \
                 só fica limitada a apagar os próprios locks quando o armazenamento aplica política \
                 por chave (AWS S3, MinIO, RustFS); no R2 e no B2 ela pode apagar tudo.",
            ),
            region = t("Region", "Região"),
            prefix = t("Prefix (may be empty)", "Prefixo (pode ficar vazio)"),
            maint_key = t("Maintenance access key", "Chave de acesso de manutenção"),
            maint_secret = t("Maintenance secret", "Segredo de manutenção"),
            device_key = t(
                "Device access key (list, read, write, own locks)",
                "Chave de acesso dos aparelhos (listar, ler, gravar, próprios locks)",
            ),
            device_secret = t("Device secret", "Segredo dos aparelhos"),
            create = t("Create", "Criar"),
            join_title = t("Join with a code", "Entrar com um código"),
            join_hint = t(
                "Open Add device on the device that holds the repository, then copy its \
                 address, code and fingerprint, or paste the whole invite.",
                "Abra Adicionar aparelho no aparelho que mantém o repositório e copie o \
                 endereço, o código e a impressão digital, ou cole o convite inteiro.",
            ),
            waiting = t(
                "Waiting for the other device to approve…",
                "Esperando o outro aparelho aprovar…",
            ),
            device_name = t("Device name", "Nome do aparelho"),
            address = t("Address", "Endereço"),
            code = t("Code", "Código"),
            fingerprint = t("Fingerprint", "Impressão digital"),
            paste = t("Or paste the invite", "Ou cole o convite"),
            join = t("Join", "Entrar"),
        ),
    )
}

/// The recovery password, once, with a copy button. Monospace and wrapping.
fn secret_block(language: Language, secret: &str) -> String {
    format!(
        "<div class=\"secret\"><code id=\"secret\">{}</code>\
         <button type=\"button\" class=\"quiet\" onclick=\"navigator.clipboard.writeText(\
         document.getElementById('secret').textContent).then(()=>{{this.textContent='{}'}})\">\
         {}</button></div>",
        escape(secret),
        language.pick("Copied", "Copiada"),
        language.pick("Copy", "Copiar"),
    )
}

/// The page frame: the palette in docs/identidade-visual.md, light or dark as the system is.
fn page(language: Language, body: &str) -> String {
    format!(
        "<!doctype html><html lang=\"{}\"><head><meta charset=\"utf-8\">\
<meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\
<meta name=\"color-scheme\" content=\"light dark\"><title>Satchel</title><style>\
:root{{--canvas:#F3EEE4;--card:#FFFCF7;--ink:#241C16;--secondary:#6B5E52;--accent:#8C4A24;\
--field:#EBE4D8;--line:rgba(36,28,22,.14);--danger:#A3261B}}\
@media (prefers-color-scheme:dark){{:root{{--canvas:#17130F;--card:#221C17;--ink:#F2ECE3;\
--secondary:#B8AB9C;--accent:#E0A070;--field:#2D261F;--line:rgba(242,236,227,.14);--danger:#F08A7E}}}}\
*{{box-sizing:border-box}}\
body{{margin:0;background:var(--canvas);color:var(--ink);font:1rem/1.5 system-ui,-apple-system,sans-serif}}\
main{{max-width:34rem;margin:0 auto;padding:1.5rem 1.25rem 3rem}}\
.brand{{display:flex;align-items:center;gap:.75rem;font-weight:600}}\
.brand img{{width:36px;height:36px;border-radius:8px}}\
h1{{font-size:2rem;font-weight:600;margin:1rem 0 .5rem;letter-spacing:-.01em}}\
h2{{font-size:1.06rem;font-weight:600;margin:0 0 .25rem}}\
p{{margin:.5rem 0}}.lead,.hint{{color:var(--secondary)}}.hint{{font-size:.94rem}}\
.card{{background:var(--card);border:1px solid var(--line);border-radius:18px;padding:1rem;margin:.75rem 0}}\
label{{display:block;margin:.75rem 0 0;font-size:.82rem;font-weight:600;color:var(--secondary)}}\
input,select,textarea{{display:block;width:100%;margin-top:.3rem;padding:.7rem .75rem;border:0;\
border-radius:12px;background:var(--field);color:var(--ink);font:400 1rem/1.4 system-ui,-apple-system,sans-serif}}\
input:focus,select:focus,textarea:focus,button:focus-visible{{outline:2px solid var(--accent);outline-offset:2px}}\
button{{display:block;width:100%;min-height:48px;margin-top:1rem;border:0;border-radius:14px;\
background:var(--accent);color:var(--card);font:inherit;font-weight:600;cursor:pointer}}\
button:disabled{{opacity:.6;cursor:progress}}\
button.quiet{{display:inline;width:auto;min-height:0;margin:0;padding:.25rem 0;background:none;color:var(--accent)}}\
.error,.warn{{color:var(--danger);font-weight:600}}\
.secret{{background:var(--card);border:1px solid var(--line);border-radius:18px;padding:1rem;margin:.75rem 0}}\
.secret code{{display:block;font:1.05rem/1.6 ui-monospace,SFMono-Regular,Menlo,monospace;overflow-wrap:anywhere}}\
code{{font-family:ui-monospace,SFMono-Regular,Menlo,monospace}}\
</style></head><body><main><div class=\"brand\"><img src=\"mark.png\" alt=\"\">Satchel</div>\
{body}</main></body></html>",
        language.html_tag()
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
    let mut accept_language = String::new();
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
        if name == "accept-language" {
            accept_language = value.to_string();
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
        accept_language,
        body: String::from_utf8_lossy(&body).to_string(),
    }))
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// The bag on dark green, the same mark as the phones.
const MARK_PNG: &[u8] = include_bytes!("../assets/satchel-mark.png");

fn respond_bytes(stream: &mut TcpStream, kind: &str, body: &[u8]) -> std::io::Result<()> {
    let head = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: {kind}\r\nContent-Length: {}\r\n\
Cache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes())?;
    stream.write_all(body)?;
    stream.flush()
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
