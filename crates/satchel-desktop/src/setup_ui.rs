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
use crate::picker::{FolderPicker, PickFor, native_picker, overlaps};
use crate::secrets::SecretStore;
use crate::text::{Language, describe};
use crate::{AccessPair, Desktop, Error, JoinTarget, add_device_config, request_grant};

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

/// Start listening on a free loopback port, with the system's folder chooser.
pub fn start<S>(desktop: Arc<Mutex<Desktop>>, secrets: Arc<S>) -> std::io::Result<SetupPage>
where
    S: SecretStore + Send + Sync + 'static,
{
    start_with_picker(desktop, secrets, native_picker())
}

/// Start listening with a given chooser. `None` hides the Choose buttons, and
/// the folders are typed.
pub fn start_with_picker<S>(
    desktop: Arc<Mutex<Desktop>>,
    secrets: Arc<S>,
    picker: Option<FolderPicker>,
) -> std::io::Result<SetupPage>
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
        picker,
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
    picker: Option<FolderPicker>,
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
        let language = Language::from_accept_language(&request.accept_language)
            .unwrap_or_else(|| lock(&shared.desktop).language());
        let body = page(
            language,
            &format!(
                "<h1>{}</h1><p class=\"lead\">{}</p>",
                language.pick("This page didn't open", "Esta página não abriu"),
                language.pick(
                    "It opens only at the address Satchel opened. Use the Satchel menu again.",
                    "Ela só abre no endereço que o Satchel abriu. Use o menu do Satchel de novo.",
                ),
            ),
        );
        return respond(&mut stream, 403, "text/html; charset=utf-8", &body);
    }
    let prefix = format!("/{}/", shared.token);
    let Some(route) = request.path.strip_prefix(&prefix) else {
        let language = Language::from_accept_language(&request.accept_language)
            .unwrap_or_else(|| lock(&shared.desktop).language());
        return not_found(&mut stream, language);
    };
    let query = route.split_once('?').map(|(_, query)| query).unwrap_or("");
    let route = route.split('?').next().unwrap_or("");
    if request.method == "GET" && route == "mark.png" {
        return respond_bytes(&mut stream, "image/png", MARK_PNG);
    }
    let language = Language::from_accept_language(&request.accept_language)
        .unwrap_or_else(|| lock(&shared.desktop).language());
    if let Some(action) = route.strip_prefix("add/") {
        return add_action(&mut stream, shared, language, &request.method, action);
    }
    if request.method == "POST" && route == "pick" {
        return pick(&mut stream, shared, language, &parse_form(&request.body));
    }
    let page = match (request.method.as_str(), route) {
        ("GET", "") => form_page(shared, language, None, &BTreeMap::new()),
        ("GET", "add") => add_page(shared, language),
        ("GET", "devices") => {
            let chosen = query.strip_prefix("remove=").unwrap_or("");
            devices_page(shared, language, chosen, None)
        }
        ("POST", "devices/remove") => remove_device(shared, language, &parse_form(&request.body)),
        ("POST", "create") => create(shared, language, &parse_form(&request.body)),
        ("POST", "recovery") => change_recovery(shared, language),
        ("POST", "join") => join(shared, language, &parse_form(&request.body)),
        (
            "POST",
            "folders/add" | "folders/receive" | "folders/stop" | "disconnect" | "main" | "sync",
        ) => manage(shared, language, route, &parse_form(&request.body)),
        _ => return not_found(&mut stream, language),
    };
    respond(&mut stream, 200, "text/html; charset=utf-8", &page)
}

/// What a change on the dashboard did, shown at the top of it.
enum Notice {
    Done(String),
    Failed(String),
}

/// The dashboard's forms: add, receive and stop a folder, and disconnect.
fn manage<S: SecretStore>(
    shared: &Shared<S>,
    language: Language,
    route: &str,
    form: &BTreeMap<String, String>,
) -> String {
    let t = |en: &'static str, pt: &'static str| language.pick(en, pt);
    let lane = || match field(form, "lane") {
        "" => None,
        id => Some(id.to_string()),
    };
    let result = {
        let mut desktop = lock(&shared.desktop);
        match route {
            "sync" => desktop
                .publish(&*shared.secrets)
                .map(|_| t("Synced.", "Sincronizado.").to_string())
                .map_err(|err| describe(language, &err)),
            "folders/add" => required_path(language, form, "path").and_then(|path| {
                let name = folder_name_of(&path, field(form, "name"));
                desktop
                    .add_folder(path, field(form, "name"))
                    .map(|()| match language {
                        Language::En => format!("“{name}” syncs from now on."),
                        Language::PtBr => format!("“{name}” sincroniza a partir de agora."),
                    })
                    .map_err(|err| describe(language, &err))
            }),
            "folders/receive" => required_path(language, form, "path").and_then(|path| {
                let name = field(form, "name").to_string();
                desktop
                    .receive_folder(lane(), &name, path)
                    .map(|()| match language {
                        Language::En => format!("“{name}” arrives here on the next sync."),
                        Language::PtBr => format!("“{name}” chega aqui na próxima sincronização."),
                    })
                    .map_err(|err| describe(language, &err))
            }),
            "main" => {
                let maintenance = if field(form, "access_key_id").is_empty() {
                    None
                } else {
                    Some(AccessPair {
                        access_key_id: field(form, "access_key_id").to_string(),
                        secret_access_key: field(form, "secret_access_key").to_string(),
                    })
                };
                desktop
                    .become_main(&*shared.secrets, field(form, "recovery"), maintenance)
                    .map(|()| {
                        t(
                            "This is now a main device: it removes devices and changes the \
                             recovery password.",
                            "Agora este é um aparelho principal: ele remove aparelhos e troca a \
                             senha de recuperação.",
                        )
                        .to_string()
                    })
                    .map_err(|err| describe(language, &err))
            }
            "folders/stop" => desktop
                .stop_folder(lane().as_deref())
                .map(|()| {
                    t(
                        "Stopped. The files stay in the folder and in the repository, and other \
                         devices keep syncing it.",
                        "Parou. Os arquivos ficam na pasta e no repositório, e os outros aparelhos \
                         continuam sincronizando.",
                    )
                    .to_string()
                })
                .map_err(|err| describe(language, &err)),
            _ => {
                let maintainer = desktop
                    .profile()
                    .is_some_and(|profile| profile.role == Role::Maintainer);
                if maintainer && field(form, "saved") != "yes" {
                    Err(t(
                        "Confirm that the recovery password is saved somewhere else first.",
                        "Confirme antes que a senha de recuperação está guardada em outro lugar.",
                    )
                    .to_string())
                } else {
                    match desktop.disconnect(&*shared.secrets) {
                        Ok(()) => {
                            drop(desktop);
                            return page(
                                language,
                                &format!(
                                    "<h1>{}</h1><p class=\"lead\">{}</p>\
                                     <p><a href=\"/{}/\">{}</a></p>",
                                    t("Disconnected", "Desconectado"),
                                    t(
                                        "This device forgot the repository. The folders and their \
                                         files are where they were, and so is the repository.",
                                        "Este aparelho esqueceu o repositório. As pastas e os \
                                         arquivos continuam onde estavam, e o repositório também.",
                                    ),
                                    shared.token,
                                    t("Set up again", "Configurar de novo"),
                                ),
                            );
                        }
                        Err(err) => Err(describe(language, &err)),
                    }
                }
            }
        }
    };
    let notice = match result {
        Ok(text) => Notice::Done(text),
        Err(text) => Notice::Failed(text),
    };
    dashboard(shared, language, Some(notice))
}

fn folder_name_of(path: &std::path::Path, typed: &str) -> String {
    match typed.trim() {
        "" => crate::folder_name(path),
        name => name.to_string(),
    }
}

/// A folder field: typed, or filled by the system chooser when there is one.
fn folder_input<S>(shared: &Shared<S>, language: Language, id: &str, target: PickFor) -> String {
    let input = format!("<input id=\"{id}\" name=\"path\">");
    if shared.picker.is_none() {
        return input;
    }
    format!(
        "<div class=\"row\">{input}<button type=\"button\" class=\"pick\" \
         data-for=\"{}\" data-target=\"{id}\">{}</button></div>",
        target.key(),
        language.pick("Choose…", "Escolher…")
    )
}

/// The page of a device that is set up: its folders, the repository's other
/// folders, the recovery password and the way out.
fn dashboard<S: SecretStore>(
    shared: &Shared<S>,
    language: Language,
    notice: Option<Notice>,
) -> String {
    let t = |en: &'static str, pt: &'static str| language.pick(en, pt);
    let base = format!("/{}/", shared.token);
    let mut desktop = lock(&shared.desktop);
    let _ = desktop.refresh_folders(&*shared.secrets);
    let Some(profile) = desktop.profile().cloned() else {
        drop(desktop);
        return form_page(shared, language, None, &BTreeMap::new());
    };
    let unreachable = desktop.unreachable_folders().clone();
    let others: Vec<satchel_core::FolderSummary> = desktop
        .repository_folders()
        .iter()
        .filter(|folder| !profile.folders.iter().any(|kept| kept.lane == folder.id))
        .cloned()
        .collect();
    let place = desktop.storage_place().unwrap_or_default();
    let local = desktop.storage_is_local_folder();
    let may_add = desktop.may_add_devices();
    let devices = desktop.devices().to_vec();
    let sync_lines = desktop.sync_lines();
    drop(desktop);

    let notice = match notice {
        Some(Notice::Done(text)) => {
            format!(
                "<p class=\"done\" role=\"status\" data-toast=\"ok\">{}</p>",
                escape(&text)
            )
        }
        Some(Notice::Failed(text)) => {
            format!(
                "<p class=\"error\" role=\"alert\" data-toast=\"error\">{}</p>",
                escape(&text)
            )
        }
        None => String::new(),
    };
    let lane_value = |lane: &Option<String>| escape(lane.as_deref().unwrap_or(""));
    let mut kept = String::new();
    if profile.folders.is_empty() {
        kept.push_str(&format!(
            "<p class=\"hint\">{}</p>",
            t(
                "No folder syncs on this device now.",
                "Nenhuma pasta sincroniza neste aparelho agora."
            )
        ));
    }
    for folder in &profile.folders {
        let status = if unreachable.contains(&folder.lane) {
            format!(
                "<p class=\"warn\">{}</p>",
                t(
                    "Not connected: paused. Connect the disk and it syncs again; nothing is deleted meanwhile.",
                    "Não está conectada: pausada. Conecte o disco e ela volta a sincronizar; nada é apagado enquanto isso.",
                )
            )
        } else {
            String::new()
        };
        kept.push_str(&format!(
            "<div class=\"folder\"><h3>{name}</h3><p class=\"hint\"><code>{path}</code></p>{status}\
             <form method=\"post\" action=\"{base}folders/stop\">\
             <input type=\"hidden\" name=\"lane\" value=\"{lane}\">\
             <button class=\"secondary\">{stop}</button></form></div>",
            name = escape(&folder.name),
            path = escape(&folder.path.display().to_string()),
            lane = lane_value(&folder.lane),
            stop = t("Stop syncing here", "Parar de sincronizar aqui"),
        ));
    }
    let mut elsewhere = String::new();
    for (index, folder) in others.iter().enumerate() {
        let name = folder
            .name
            .clone()
            .unwrap_or_else(|| t("Satchel", "Satchel").to_string());
        let id = format!("receive-{index}");
        let count = folder.devices.len();
        let devices = match (language, count) {
            (Language::En, 1) => "kept on 1 device".to_string(),
            (Language::En, n) => format!("kept on {n} devices"),
            (Language::PtBr, 1) => "mantida em 1 aparelho".to_string(),
            (Language::PtBr, n) => format!("mantida em {n} aparelhos"),
        };
        elsewhere.push_str(&format!(
            "<div class=\"folder\"><h3>{name_html}</h3><p class=\"hint\">{devices}</p>\
             <form method=\"post\" action=\"{base}folders/receive\">\
             <input type=\"hidden\" name=\"lane\" value=\"{lane}\">\
             <input type=\"hidden\" name=\"name\" value=\"{name_html}\">\
             <label for=\"{id}\">{where_label}</label>{field}\
             <button>{receive}</button></form></div>",
            name_html = escape(&name),
            lane = lane_value(&folder.id),
            where_label = t(
                "Where to keep it on this computer",
                "Onde manter neste computador"
            ),
            field = folder_input(shared, language, &id, PickFor::Synced),
            receive = t("Receive on this computer", "Receber neste computador"),
        ));
    }
    let elsewhere = if elsewhere.is_empty() {
        String::new()
    } else {
        format!(
            "<section class=\"card\"><h2>{}</h2><p class=\"hint\">{}</p>{elsewhere}</section>",
            t(
                "Other folders of the repository",
                "Outras pastas do repositório"
            ),
            t(
                "Other devices sync these. Choose where each one goes to receive it here too.",
                "Outros aparelhos sincronizam estas. Escolha onde cada uma fica para receber aqui também.",
            ),
        )
    };
    let recovery = if profile.role == Role::Maintainer {
        format!(
            "<section class=\"card\"><h2>{title}</h2><p class=\"hint\">{hint}</p>\
             <button type=\"button\" class=\"secondary\" data-reveal=\"recovery-confirm\" \
             aria-expanded=\"false\" aria-controls=\"recovery-confirm\">{ask}</button>\
             <form method=\"post\" action=\"{base}recovery\" id=\"recovery-confirm\" hidden \
             onsubmit=\"busy(this,'{changing}')\"><p class=\"warn\">{consequence}</p>\
             <label class=\"check\"><input type=\"checkbox\" required> {ready}</label>\
             <button class=\"danger\">{change}</button>\
             <button type=\"button\" class=\"secondary\" data-hide=\"recovery-confirm\">{cancel}</button>\
             </form></section>",
            title = t("Recovery password", "Senha de recuperação"),
            hint = t(
                "Replace it if it leaked or you lost track of it. Devices keep working. \
                 You get the new one once.",
                "Troque se ela vazou ou se você perdeu o controle dela. Os aparelhos continuam \
                 funcionando. Você vê a nova uma vez só.",
            ),
            ask = t("Change recovery password…", "Trocar senha de recuperação…"),
            consequence = t(
                "The current recovery password stops opening the repository, also in the official \
                 restic. The new one appears once, on the next page.",
                "A senha de recuperação atual deixa de abrir o repositório, também no restic \
                 oficial. A nova aparece uma vez só, na próxima página.",
            ),
            ready = t(
                "I can save the new one in my password manager now.",
                "Consigo guardar a nova no meu gerenciador de senhas agora.",
            ),
            changing = t("Changing…", "Trocando…"),
            change = t("Change now", "Trocar agora"),
            cancel = t("Cancel", "Cancelar"),
        )
    } else {
        String::new()
    };
    let saved = if profile.role == Role::Maintainer {
        format!(
            "<label class=\"check\"><input type=\"checkbox\" name=\"saved\" value=\"yes\" required> {}</label>",
            t(
                "The recovery password is saved somewhere else. Without it, nobody adds devices \
                 or restores the repository.",
                "A senha de recuperação está guardada em outro lugar. Sem ela, ninguém adiciona \
                 aparelhos nem restaura o repositório.",
            )
        )
    } else {
        String::new()
    };
    let local_note = if local {
        format!(
            "<p class=\"hint\">{}</p>",
            t(
                "The encrypted copies are in a folder on this computer, so Add device is off: \
                 phones and other computers can't reach it.",
                "As cópias cifradas ficam numa pasta deste computador, então Adicionar aparelho \
                 está desligado: celulares e outros computadores não alcançam essa pasta.",
            )
        )
    } else {
        String::new()
    };
    let status = status_card(language, &sync_lines, may_add, &base);
    page(
        language,
        &format!(
            "<h1>{title}</h1>{notice}
{status}
<section class=\"card\"><h2>{folders_title}</h2>{kept}</section>
{elsewhere}
{devices_card}
<section class=\"card\"><h2>{add_title}</h2><p class=\"hint\">{add_hint}</p>
<form method=\"post\" action=\"{base}folders/add\">
<label for=\"new-folder\">{folder_label}</label>{new_field}
<label>{name_label} <input name=\"name\" placeholder=\"{name_hint}\"></label>
<button class=\"secondary\">{add}</button></form></section>
<section class=\"card\"><h2>{storage_title}</h2><p><code>{place}</code></p>{local_note}</section>
{recovery}
{main_card}
<section class=\"card\"><h2>{leave_title}</h2><p class=\"hint\">{leave_hint}</p>
<button type=\"button\" class=\"secondary\" id=\"leave-open\" aria-expanded=\"false\" aria-controls=\"leave-confirm\">{leave_ask}</button>
<form method=\"post\" action=\"{base}disconnect\" id=\"leave-confirm\" hidden>
<p class=\"warn\">{leave_confirm}</p>{saved}
<button class=\"danger\">{leave}</button>
<button type=\"button\" class=\"secondary\" id=\"leave-cancel\">{cancel}</button></form></section>
<script>
function busy(f,t){{const b=f.querySelector('button:not(.pick)');b.disabled=true;b.textContent=t;b.setAttribute('aria-busy','true');}}
for(const b of document.querySelectorAll('button.pick')){{b.addEventListener('click',async()=>{{
const label=b.textContent;b.disabled=true;b.textContent='{choosing}';
try{{const r=await fetch('{base}pick',{{method:'POST',headers:{{'Content-Type':'application/x-www-form-urlencoded'}},body:'for='+b.dataset.for}});
if(r.status===200){{const i=document.getElementById(b.dataset.target);i.value=await r.text();i.focus();}}}}
else if(r.status!==204){{toast('{pick_failed}','error');}}}}
finally{{b.disabled=false;b.textContent=label;}}}});}}
for(const b of document.querySelectorAll('[data-reveal]')){{b.addEventListener('click',()=>{{const box=document.getElementById(b.dataset.reveal);box.hidden=false;b.hidden=true;b.setAttribute('aria-expanded','true');box.querySelector('input,button').focus();}});}}
for(const b of document.querySelectorAll('[data-hide]')){{b.addEventListener('click',()=>{{const box=document.getElementById(b.dataset.hide);box.hidden=true;const o=document.querySelector('[data-reveal=\"'+b.dataset.hide+'\"]');o.hidden=false;o.setAttribute('aria-expanded','false');o.focus();}});}}
const leaveOpen=document.getElementById('leave-open'),leaveBox=document.getElementById('leave-confirm');
leaveOpen.addEventListener('click',()=>{{leaveBox.hidden=false;leaveOpen.hidden=true;leaveOpen.setAttribute('aria-expanded','true');leaveBox.querySelector('button.danger').focus();}});
document.getElementById('leave-cancel').addEventListener('click',()=>{{leaveBox.hidden=true;leaveOpen.hidden=false;leaveOpen.setAttribute('aria-expanded','false');leaveOpen.focus();}});
</script>",
            devices_card = devices_card(language, &profile, &devices),
            main_card = main_card(shared, language, &profile),
            leave_ask = t("Disconnect…", "Desconectar…"),
            cancel = t("Cancel", "Cancelar"),
            pick_failed = t("The folder chooser didn't open. Type the path.", "O seletor de pastas não abriu. Digite o caminho."),
            title = t("This device", "Este aparelho"),
            folders_title = t("Folders on this computer", "Pastas neste computador"),
            add_title = t("Add a folder", "Adicionar uma pasta"),
            add_hint = t(
                "It goes to the same repository, with the same keys. Your other devices see it \
                 and choose whether to receive it.",
                "Ela vai para o mesmo repositório, com as mesmas chaves. Os outros aparelhos a \
                 veem e escolhem se querem recebê-la.",
            ),
            folder_label = t("Folder to sync", "Pasta para sincronizar"),
            new_field = folder_input(shared, language, "new-folder", PickFor::Synced),
            name_label = t("Name (optional)", "Nome (opcional)"),
            name_hint = t("The folder's own name", "O nome da própria pasta"),
            add = t("Add folder", "Adicionar pasta"),
            storage_title = t("Encrypted copies", "Cópias cifradas"),
            place = escape(&place),
            leave_title = t("Disconnect this device", "Desconectar este aparelho"),
            leave_hint = t(
                "This device forgets the repository and its keys. The folders, their files and \
                 the repository stay. To use another storage, disconnect and set up again.",
                "Este aparelho esquece o repositório e as chaves. As pastas, os arquivos e o \
                 repositório ficam. Para usar outro armazenamento, desconecte e configure de novo.",
            ),
            leave_confirm = t(
                "This device stops syncing every folder and forgets its keys.",
                "Este aparelho para de sincronizar todas as pastas e esquece as chaves.",
            ),
            leave = t("Disconnect", "Desconectar"),
            choosing = t("Choosing…", "Escolhendo…"),
        ),
    )
}

/// The top of the dashboard: how the last sync on this device ended, and the way to add a device.
fn status_card(language: Language, lines: &[crate::SyncLine], may_add: bool, base: &str) -> String {
    let t = |en: &'static str, pt: &'static str| language.pick(en, pt);
    let mut rows = String::new();
    for (index, line) in lines.iter().enumerate() {
        let class = match (index, line.tone) {
            (_, crate::Tone::Warn) => "warn",
            (0, _) => "state",
            _ => "hint",
        };
        // The first line carries a dot: green when it synced, red when it failed. The words
        // say the same, so the color is never the only sign.
        let dot = match (index, line.tone) {
            (0, crate::Tone::Good) => "<span class=\"dot ok\" aria-hidden=\"true\"></span>",
            (0, crate::Tone::Warn) => "<span class=\"dot bad\" aria-hidden=\"true\"></span>",
            (0, crate::Tone::Plain) => "<span class=\"dot\" aria-hidden=\"true\"></span>",
            _ => "",
        };
        rows.push_str(&format!(
            "<p class=\"{class}\">{dot}{}</p>",
            escape(&line.text)
        ));
    }
    let add = if !may_add {
        String::new()
    } else {
        format!(
            "<a class=\"button\" href=\"{base}add\">{}</a>",
            t("Add device", "Adicionar aparelho")
        )
    };
    format!(
        "<section class=\"card\" aria-labelledby=\"sync-title\"><h2 id=\"sync-title\">{title}</h2>\
         {rows}<p class=\"hint\">{how}</p>\
         <form method=\"post\" action=\"{base}sync\" onsubmit=\"busy(this,'{syncing}')\">\
         <button class=\"secondary\">{sync}</button></form>{add}</section>",
        syncing = t("Syncing…", "Sincronizando…"),
        sync = t("Sync now", "Sincronizar agora"),
        title = t("Sync on this device", "Sincronização neste aparelho"),
        how = t(
            "The Satchel menu syncs every two minutes and shows the same state. This page shows \
             it as of when it opened.",
            "O menu do Satchel sincroniza a cada dois minutos e mostra o mesmo estado. Esta \
             página mostra como estava quando abriu.",
        ),
    )
}

/// The repository's devices: who each is, when it last synced, and which is main.
fn devices_card(
    language: Language,
    profile: &crate::Profile,
    devices: &[satchel_core::DeviceCard],
) -> String {
    let t = |en: &'static str, pt: &'static str| language.pick(en, pt);
    let mut rows = String::new();
    for card in devices {
        let name = card.name.clone().unwrap_or_else(|| {
            t("A device with no name yet", "Um aparelho ainda sem nome").to_string()
        });
        let mut badges = String::new();
        if card.main {
            badges.push_str(&format!(
                "<span class=\"badge\">{}</span>",
                t("main", "principal")
            ));
        }
        if card.id == profile.device_id.as_str() {
            badges.push_str(&format!(
                "<span class=\"badge quiet\">{}</span>",
                t("this one", "este")
            ));
        }
        let os = card.os.as_deref().map(os_label).unwrap_or_default();
        let seen = crate::app::local_clock(u64::try_from(card.last_unix).unwrap_or(0));
        rows.push_str(&format!(
            "<div class=\"folder\"><h3>{} {badges}</h3><p class=\"hint\">{}{}{} {}</p></div>",
            escape(&name),
            escape(&os),
            if os.is_empty() { "" } else { " · " },
            t("last sync", "última sincronização"),
            escape(&seen),
        ));
    }
    if rows.is_empty() {
        rows = format!(
            "<p class=\"hint\">{}</p>",
            t(
                "No device has synced yet.",
                "Nenhum aparelho sincronizou ainda."
            )
        );
    }
    let hint = match profile.role {
        Role::Maintainer => t(
            "This is a main device: it adds and removes devices and changes the recovery \
             password. You choose which devices are main: any one becomes main with the recovery \
             password and the maintenance key.",
            "Este é um aparelho principal: ele adiciona e remove aparelhos e troca a senha de \
             recuperação. Você escolhe quais aparelhos são principais: qualquer um vira principal \
             com a senha de recuperação e a chave de manutenção.",
        ),
        Role::Member => t(
            "Adding or removing a device, and changing the recovery password, is done on a main \
             device. This one becomes main with the recovery password and the maintenance key, \
             below.",
            "Adicionar ou remover aparelhos, e trocar a senha de recuperação, se faz num aparelho \
             principal. Este vira principal com a senha de recuperação e a chave de manutenção, \
             aqui embaixo.",
        ),
    };
    let remove = if profile.role == Role::Maintainer && profile.object_store.is_some() {
        format!(
            "<a class=\"button secondary\" href=\"devices\">{}</a>",
            t("Remove a device…", "Remover um aparelho…")
        )
    } else {
        String::new()
    };
    format!(
        "<section class=\"card\"><h2>{}</h2><p class=\"hint\">{hint}</p>{rows}{remove}</section>",
        t("Devices", "Aparelhos")
    )
}

/// What removing a device does and does not do, said before it is done.
fn removal_consequence(language: Language) -> (&'static str, &'static str) {
    (
        language.pick(
            "Removing deletes its key file: its password no longer opens the repository. It keeps \
             the copies it already opened.",
            "Remover apaga o arquivo de chave dele: a senha dele deixa de abrir o repositório. Ele \
             fica com as cópias que já abriu.",
        ),
        language.pick(
            "It keeps the bucket access key, which all devices share: until you replace that key in \
             the storage's console, it can still list, read and write in the bucket, and on R2 and \
             B2 also delete. After the replacement the other devices join again.",
            "Ele continua com a chave de acesso ao bucket, que é a mesma em todos os aparelhos: até \
             você trocar essa chave no painel do armazenamento, ele ainda lista, lê e grava no \
             bucket, e no R2 e no B2 também apaga. Depois da troca, os outros aparelhos entram de novo.",
        ),
    )
}

/// The devices a main device can remove, each with its name when it published one.
fn devices_page<S: SecretStore>(
    shared: &Shared<S>,
    language: Language,
    chosen: &str,
    notice: Option<Notice>,
) -> String {
    let t = |en: &'static str, pt: &'static str| language.pick(en, pt);
    let base = format!("/{}/", shared.token);
    let title = t("Remove a device", "Remover um aparelho");
    let mut desktop = lock(&shared.desktop);
    let Some(profile) = desktop.profile().cloned() else {
        drop(desktop);
        return form_page(shared, language, None, &BTreeMap::new());
    };
    let notice = match notice {
        Some(Notice::Done(text)) => {
            format!("<p class=\"done\" role=\"status\">{}</p>", escape(&text))
        }
        Some(Notice::Failed(text)) => {
            format!("<p class=\"error\" role=\"alert\">{}</p>", escape(&text))
        }
        None => String::new(),
    };
    let back = format!(
        "<p><a href=\"{base}\">{}</a></p>",
        t("Back to this device", "Voltar para este aparelho")
    );
    if profile.role != Role::Maintainer {
        drop(desktop);
        return page(
            language,
            &format!(
                "<h1>{title}</h1><section class=\"card\"><p>{}</p></section>{back}",
                t(
                    "Only a main device removes devices.",
                    "Só um aparelho principal remove aparelhos."
                )
            ),
        );
    }
    if let Err(err) = desktop.load_device_keys(&*shared.secrets) {
        drop(desktop);
        return page(
            language,
            &format!(
                "<h1>{title}</h1>{notice}<section class=\"card\"><p class=\"error\" role=\"alert\">{}</p></section>{back}",
                escape(&describe(language, &err))
            ),
        );
    }
    let keys = desktop.device_keys().map(<[_]>::to_vec).unwrap_or_default();
    let cards = desktop.devices().to_vec();
    let bucket_keys: Vec<crate::BucketKey> = keys
        .iter()
        .map(|key| desktop.bucket_key_of(&*shared.secrets, key.device_id.as_str()))
        .collect();
    drop(desktop);
    let mut rows = String::new();
    for (key, bucket_key) in keys.iter().zip(&bucket_keys) {
        let card = cards.iter().find(|card| card.id == key.device_id.as_str());
        let bucket_line = match bucket_key {
            crate::BucketKey::Own => format!(
                "<p class=\"hint\">{}</p>",
                t(
                    "It has its own bucket key: removing it deletes that key too, and it loses \
                     the bucket.",
                    "Ele tem chave própria no bucket: remover apaga essa chave também, e ele perde \
                     o acesso ao bucket.",
                )
            ),
            crate::BucketKey::Shared => format!(
                "<p class=\"warn\">{}</p>",
                escape(removal_consequence(language).1)
            ),
            crate::BucketKey::Unknown => format!(
                "<p class=\"warn\">{} {}</p>",
                t(
                    "The maintenance key couldn't check its bucket key.",
                    "A chave de manutenção não conseguiu conferir a chave dele no bucket.",
                ),
                escape(removal_consequence(language).1)
            ),
        };
        let name = card.and_then(|card| card.name.clone()).unwrap_or_else(|| {
            t("A device with no name yet", "Um aparelho ainda sem nome").to_string()
        });
        let mut facts = Vec::new();
        if let Some(os) = card.and_then(|card| card.os.as_deref()) {
            facts.push(os_label(os));
        }
        if let Some(card) = card {
            facts.push(format!(
                "{} {}",
                t("last sync", "última sincronização"),
                crate::app::local_clock(u64::try_from(card.last_unix).unwrap_or(0))
            ));
        }
        facts.push(format!(
            "id {}",
            grouped_fingerprint(key.device_id.as_str())
        ));
        let open = if key.key_id == chosen { " open" } else { "" };
        rows.push_str(&format!(
            "<details class=\"folder\"{open}><summary>{name}</summary><p class=\"hint\">{facts}</p>{bucket_line}\
             <form method=\"post\" action=\"{base}devices/remove\" onsubmit=\"busy(this,'{removing}')\">\
             <input type=\"hidden\" name=\"key\" value=\"{key_id}\">\
             <label class=\"check\"><input type=\"checkbox\" required> {confirm}</label>\
             <button class=\"danger\">{remove}</button></form></details>",
            name = escape(&name),
            facts = escape(&facts.join(" · ")),
            key_id = escape(&key.key_id),
            removing = t("Removing…", "Removendo…"),
            confirm = t(
                "I read what removing does and does not do.",
                "Li o que remover faz e o que não faz.",
            ),
            remove = t("Remove this device", "Remover este aparelho"),
        ));
    }
    if rows.is_empty() {
        rows = format!(
            "<p class=\"hint\">{}</p>",
            t(
                "No other device has a key.",
                "Nenhum outro aparelho tem chave."
            )
        );
    }
    page(
        language,
        &format!(
            "<h1>{title}</h1>{notice}<section class=\"card\"><p>{does}</p>{rows}</section>{back}
<script>function busy(f,t){{const b=f.querySelector('button');b.disabled=true;b.textContent=t;b.setAttribute('aria-busy','true');}}</script>",
            does = escape(removal_consequence(language).0),
        ),
    )
}

fn remove_device<S: SecretStore>(
    shared: &Shared<S>,
    language: Language,
    form: &BTreeMap<String, String>,
) -> String {
    let key = field(form, "key");
    let mut had_own_key = false;
    let result = {
        let mut desktop = lock(&shared.desktop);
        if desktop.device_keys().is_none() {
            let _ = desktop.load_device_keys(&*shared.secrets);
        }
        if let Some(device_id) = desktop
            .device_keys()
            .and_then(|keys| keys.iter().find(|record| record.key_id == key))
            .map(|record| record.device_id.as_str().to_string())
        {
            had_own_key =
                desktop.bucket_key_of(&*shared.secrets, &device_id) == crate::BucketKey::Own;
        }
        let maintainer = desktop
            .profile()
            .is_some_and(|profile| profile.role == Role::Maintainer);
        if !maintainer {
            Err(language
                .pick(
                    "Only a main device removes devices.",
                    "Só um aparelho principal remove aparelhos.",
                )
                .to_string())
        } else {
            desktop
                .revoke_device_key(&*shared.secrets, key)
                .map_err(|err| describe(language, &err))
        }
    };
    let notice = match result {
        Ok(()) if had_own_key => Notice::Done(
            language
                .pick(
                    "Removed: its key no longer opens the repository, and its bucket key was deleted.",
                    "Removido: a chave dele não abre mais o repositório, e a chave dele no bucket foi \
                     apagada.",
                )
                .to_string(),
        ),
        Ok(()) => Notice::Done(
            language
                .pick(
                    "Removed: its key no longer opens the repository. Replace the devices' bucket \
                     key in the storage's console to cut its access to the bucket too.",
                    "Removido: a chave dele não abre mais o repositório. Troque a chave dos \
                     aparelhos no painel do armazenamento para cortar também o acesso ao bucket.",
                )
                .to_string(),
        ),
        Err(text) => Notice::Failed(text),
    };
    devices_page(shared, language, "", Some(notice))
}

/// On a device that joined: become a main device with the recovery password and,
/// for a bucket, the maintenance key. For when the main device is gone.
fn main_card<S>(shared: &Shared<S>, language: Language, profile: &crate::Profile) -> String {
    let t = |en: &'static str, pt: &'static str| language.pick(en, pt);
    if profile.role == Role::Maintainer {
        return String::new();
    }
    let base = format!("/{}/", shared.token);
    let keys = if profile.object_store.is_some() {
        format!(
            "<label>{} <input name=\"access_key_id\" autocomplete=\"off\" required></label>\
             <label>{} <input name=\"secret_access_key\" type=\"password\" autocomplete=\"off\" required></label>",
            t("Maintenance access key", "Chave de acesso de manutenção"),
            t("Maintenance secret", "Segredo de manutenção"),
        )
    } else {
        String::new()
    };
    format!(
        "<section class=\"card\"><h2>{title}</h2><p class=\"hint\">{hint}</p>\
         <form method=\"post\" action=\"{base}main\" onsubmit=\"busy(this,'{checking}')\">\
         <label>{recovery} <input name=\"recovery\" type=\"password\" autocomplete=\"off\" required></label>{keys}\
         <button class=\"secondary\">{make}</button></form></section>",
        title = t(
            "Make this a main device",
            "Tornar este um aparelho principal"
        ),
        hint = t(
            "For when the main device is lost or far away. Type the recovery password and the \
             maintenance key; both are checked against the repository before anything is kept.",
            "Para quando o aparelho principal se perdeu ou está longe. Digite a senha de \
             recuperação e a chave de manutenção; as duas são conferidas no repositório antes \
             de qualquer coisa ser guardada.",
        ),
        checking = t("Checking…", "Conferindo…"),
        recovery = t("Recovery password", "Senha de recuperação"),
        make = t(
            "Make this a main device",
            "Tornar este um aparelho principal"
        ),
    )
}

/// A page for an address that is not there, in the person's language.
fn not_found(stream: &mut TcpStream, language: Language) -> std::io::Result<()> {
    let body = page(
        language,
        &format!(
            "<h1>{}</h1><p class=\"lead\">{}</p>",
            language.pick("This page is gone", "Esta página não existe mais"),
            language.pick(
                "Satchel opens a new address each time it starts. Open the page again from the \
                 Satchel menu.",
                "O Satchel abre um endereço novo cada vez que começa. Abra a página de novo pelo \
                 menu do Satchel.",
            ),
        ),
    );
    respond(stream, 404, "text/html; charset=utf-8", &body)
}

/// The Add device page: opens the invite if it is not open, and shows it large.
fn add_page<S: SecretStore>(shared: &Shared<S>, language: Language) -> String {
    let t = |en: &'static str, pt: &'static str| language.pick(en, pt);
    let mut desktop = lock(&shared.desktop);
    let title = t("Add device", "Adicionar aparelho");
    let refuse = |text: &str| {
        page(
            language,
            &format!(
                "<h1>{title}</h1><section class=\"card\"><p>{}</p></section>",
                escape(text)
            ),
        )
    };
    if desktop.profile().is_none() {
        return refuse(t(
            "Set up this device first: choose Create or join in the Satchel menu.",
            "Configure este aparelho primeiro: escolha Criar ou entrar no menu do Satchel.",
        ));
    }
    if desktop.offer().is_none() {
        // Nobody else reaches a folder on this computer: no invite is opened for it.
        if desktop.storage_is_local_folder() {
            return refuse(&describe(language, &Error::LocalRepository));
        }
        if let Err(err) = desktop.open_add_device(add_device_config()) {
            return refuse(&describe(language, &err));
        }
        desktop.clear_problem();
    }
    let Some(offer) = desktop.offer().cloned() else {
        return refuse(&describe(
            language,
            &Error::Pair(satchel_pair::Error::Closed),
        ));
    };
    let local = if desktop.storage_is_local_folder() {
        format!(
            "<section class=\"card\"><p class=\"warn\">{}</p><p class=\"hint\">{}</p></section>",
            escape(&match language {
                Language::En => format!(
                    "The repository is a folder on this computer ({}).",
                    desktop.storage_place().unwrap_or_default()
                ),
                Language::PtBr => format!(
                    "O repositório é uma pasta deste computador ({}).",
                    desktop.storage_place().unwrap_or_default()
                ),
            }),
            t(
                "A phone can't reach it, so the phone would join and then fail to open the \
                 files. To use a phone, create the repository again on S3, R2 or B2.",
                "Um celular não alcança essa pasta: ele entraria e depois não abriria os \
                 arquivos. Para usar o celular, crie o repositório de novo em S3, R2 ou B2.",
            ),
        )
    } else {
        String::new()
    };
    drop(desktop);
    let base = format!("/{}/", shared.token);
    page(
        language,
        &format!(
            "<h1>{title}</h1>{local}
<section class=\"card\" id=\"waiting\"><h2>{scan_title}</h2>
<p class=\"hint\">{scan_hint}</p>
<img class=\"qr\" src=\"{base}add/qr.png?{alias}\" alt=\"{qr_alt}\">
<p class=\"hint\">{type_hint}</p>
<p class=\"code\" aria-label=\"{code_label}\">{code}</p>
<p><span class=\"hint\">{address_label}</span> <code>{address}</code></p>
<p><span class=\"hint\">{fingerprint_label}</span> <code>{fingerprint}</code></p>
<p class=\"hint\">{expires}</p>
<p class=\"hint\">{copy_hint}</p>
<textarea id=\"invite\" hidden readonly>{invite}</textarea>
<button type=\"button\" class=\"secondary\" onclick=\"navigator.clipboard.writeText(document.getElementById('invite').value).then(()=>toast('{copied}','ok'),()=>toast('{copy_failed}','error'))\">{copy}</button>
<button type=\"button\" class=\"secondary\" onclick=\"act('stop',this)\">{stop}</button></section>
<section class=\"card\" id=\"pending\" hidden><h2 id=\"who\"></h2>
<p id=\"what\" class=\"hint\"></p>
<p>{here_label} <strong class=\"code-inline\">{code}</strong></p>
<p class=\"hint\">{check}</p>
<button type=\"button\" onclick=\"act('approve',this)\">{approve}</button>
<button type=\"button\" class=\"secondary\" onclick=\"act('deny',this)\">{deny}</button></section>
<section class=\"card\" id=\"done\" hidden role=\"status\"><p id=\"result\"></p>
<button type=\"button\" onclick=\"location.reload()\">{again}</button></section>
<script>
const base='{base}';let stopped=false;
const words={{joins:'{joins}',system:'{system}',from:'{from}',typed:'{typed}',approved:'{approved}',approvedShared:'{approved_shared}',denied:'{denied}',closed:'{closed}'}};
function show(id){{for(const s of ['waiting','pending','done'])document.getElementById(s).hidden=s!==id;}}
function finish(text,bad){{stopped=true;const r=document.getElementById('result');r.textContent=text;r.className=bad?'error':'';show('done');}}
async function poll(){{if(stopped)return;try{{const r=await fetch(base+'add/state');const s=await r.json();
if(stopped)return;
if(s.state==='pending'){{document.getElementById('who').textContent=words.joins.replace('%',s.name);
document.getElementById('what').textContent=words.system+' '+s.os+' · '+words.from+' '+s.from+' · '+words.typed+' '+s.code;show('pending');}}
else if(s.state==='closed'){{finish(s.problem||words.closed,!!s.problem);return;}}}}catch(e){{}}setTimeout(poll,1000);}}
async function act(what,button){{stopped=true;for(const b of document.querySelectorAll('button'))b.disabled=true;
button.textContent=what==='approve'?'{approving}':what==='deny'?'{denying}':'{stopping}';button.setAttribute('aria-busy','true');
try{{const r=await fetch(base+'add/'+what,{{method:'POST'}});const s=await r.json();
if(!s.ok){{finish(s.message,true);return;}}
finish(what==='approve'?(s.sharedKey?words.approvedShared:words.approved):what==='deny'?words.denied:words.closed,false);}}
finally{{for(const b of document.querySelectorAll('button'))b.disabled=false;}}}}
poll();
</script>",
            scan_title = t("On the phone: scan this code", "No celular: leia este código"),
            scan_hint = t(
                "Open Satchel on the phone and choose to scan the code.",
                "Abra o Satchel no celular e escolha ler o código.",
            ),
            alias = escape(&offer.alias),
            qr_alt = t("Invite code to scan", "Código do convite para ler"),
            type_hint = t(
                "On a computer, or without the camera, type this code:",
                "Num computador, ou sem câmera, digite este código:",
            ),
            code_label = t("Code", "Código"),
            code = escape(&offer.code),
            address_label = t("Address:", "Endereço:"),
            address = escape(&offer.address.to_string()),
            fingerprint_label = t("Fingerprint:", "Impressão digital:"),
            fingerprint = escape(&grouped_fingerprint(&offer.fingerprint)),
            expires = t(
                "The invite lasts 3 minutes. Both devices need to be on the same network.",
                "O convite vale por 3 minutos. Os dois aparelhos precisam estar na mesma rede.",
            ),
            stop = t("Stop adding", "Parar de adicionar"),
            check = t(
                "Approve only if this is your device and the code it typed is the one on this \
                 screen.",
                "Aprove só se este for o seu aparelho e o código digitado for o desta tela.",
            ),
            approve = t("Approve", "Aprovar"),
            approving = t("Approving…", "Aprovando…"),
            denying = t("Denying…", "Recusando…"),
            stopping = t("Stopping…", "Parando…"),
            here_label = t("Code on this screen:", "Código desta tela:"),
            invite = escape(&offer.invite),
            copy = t("Copy invite", "Copiar convite"),
            copied = t("Invite copied.", "Convite copiado."),
            copy_failed = t(
                "Couldn't copy. Type the code, the address and the fingerprint instead.",
                "Não deu para copiar. Digite o código, o endereço e a impressão digital.",
            ),
            copy_hint = t(
                "On another computer you can also paste the whole invite in Join, instead of \
                 typing the three fields.",
                "Noutro computador você também pode colar o convite inteiro em Entrar, em vez de \
                 digitar os três campos.",
            ),
            deny = t("Deny", "Recusar"),
            again = t("Add another device", "Adicionar outro aparelho"),
            joins = t("“%” wants to join", "“%” quer entrar"),
            system = t("System:", "Sistema:"),
            from = t("from", "de"),
            typed = t("code typed:", "código digitado:"),
            approved = t(
                "Done. The device received what it needs and is opening the repository.",
                "Pronto. O aparelho recebeu o que precisa e está abrindo o repositório.",
            ),
            approved_shared = t(
                "Done. The device is opening the repository. It got the bucket key all devices \
                 share, because the maintenance key can't make keys: removing it later won't cut \
                 its bucket access.",
                "Pronto. O aparelho está abrindo o repositório. Ele recebeu a chave do bucket que \
                 todos os aparelhos dividem, porque a chave de manutenção não cria chaves: \
                 removê-lo depois não corta o acesso dele ao bucket.",
            ),
            denied = t("Denied. Nothing was sent.", "Recusado. Nada foi enviado."),
            closed = t("Add device is closed.", "Adicionar aparelho foi encerrado."),
        ),
    )
}

/// The system as people write it: “macOS”, not Rust's “macos”.
fn os_label(os: &str) -> String {
    match os {
        "macos" => "macOS".to_string(),
        "ios" => "iOS".to_string(),
        "linux" => "Linux".to_string(),
        "android" => "Android".to_string(),
        "windows" => "Windows".to_string(),
        other => other.to_string(),
    }
}

fn grouped_fingerprint(hex: &str) -> String {
    let chars: Vec<char> = hex.chars().collect();
    chars
        .chunks(4)
        .map(|chunk| chunk.iter().collect::<String>())
        .collect::<Vec<_>>()
        .join(" ")
}

/// The live parts of the Add device page: the QR, the state, and the decisions.
fn add_action<S: SecretStore>(
    stream: &mut TcpStream,
    shared: &Shared<S>,
    language: Language,
    method: &str,
    action: &str,
) -> std::io::Result<()> {
    let json = |stream: &mut TcpStream, value: serde_json::Value| {
        respond(stream, 200, "application/json", &value.to_string())
    };
    let mut desktop = lock(&shared.desktop);
    match (method, action) {
        ("GET", "qr.png") => {
            let png = desktop
                .offer()
                .and_then(|offer| satchel_pair::Invite::decode(&offer.invite).ok())
                .and_then(|invite| satchel_pair::invite_qr_png(&invite).ok());
            drop(desktop);
            match png {
                Some(png) => respond_bytes(stream, "image/png", &png),
                None => not_found(stream, language),
            }
        }
        ("GET", "state") => {
            // The page asks every second; it must not wait for the tray's own timer.
            desktop.poll();
            let value = if let Some(pending) = desktop.pending() {
                serde_json::json!({
                    "state": "pending",
                    "name": pending.device_name,
                    "os": os_label(&pending.os),
                    "from": pending.peer_address.ip().to_string(),
                    "code": pending.code,
                })
            } else if desktop.offer().is_some() {
                serde_json::json!({ "state": "waiting" })
            } else {
                match desktop.problem() {
                    Some(problem) => serde_json::json!({ "state": "closed", "problem": problem }),
                    None => serde_json::json!({ "state": "closed" }),
                }
            };
            drop(desktop);
            json(stream, value)
        }
        ("POST", "approve" | "deny" | "stop") => {
            let result = match action {
                "approve" => desktop.approve_pending(&*shared.secrets),
                "deny" => desktop.deny_pending(),
                _ => {
                    desktop.close_add_device();
                    Ok(())
                }
            };
            let value = match result {
                Ok(()) => {
                    desktop.clear_problem();
                    let shared_key = action == "approve"
                        && desktop.per_device_bucket_keys()
                        && desktop.last_grant_key() != Some(crate::BucketKey::Own);
                    serde_json::json!({ "ok": true, "sharedKey": shared_key })
                }
                Err(err) => {
                    desktop.report_problem(&err);
                    serde_json::json!({ "ok": false, "message": describe(language, &err) })
                }
            };
            drop(desktop);
            json(stream, value)
        }
        _ => {
            drop(desktop);
            not_found(stream, language)
        }
    }
}

/// Opens the chooser and answers with the folder: 200 and the path, 204 when
/// the person cancelled, 404 when this desktop has no chooser.
fn pick<S>(
    stream: &mut TcpStream,
    shared: &Shared<S>,
    language: Language,
    form: &BTreeMap<String, String>,
) -> std::io::Result<()> {
    let (Some(picker), Some(target)) = (&shared.picker, PickFor::parse(field(form, "for"))) else {
        return respond(stream, 404, "text/plain", "not found");
    };
    match picker(language, target) {
        Some(path) => respond(
            stream,
            200,
            "text/plain; charset=utf-8",
            &path.to_string_lossy(),
        ),
        None => respond(stream, 204, "text/plain", ""),
    }
}

fn change_recovery<S: SecretStore>(shared: &Shared<S>, language: Language) -> String {
    let t = |en: &'static str, pt: &'static str| language.pick(en, pt);
    let mut desktop = lock(&shared.desktop);
    match desktop.change_recovery_password(&*shared.secrets) {
        Ok(password) => page(
            language,
            &format!(
                "<h1>{}</h1><p>{}</p>{}<p class=\"hint\">{}</p>{}",
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
                saved_button(language, &format!("/{}/", shared.token)),
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
        if overlaps(&folder, &setup.directory) {
            return Err(t(
                "The folder for the encrypted copies can't be inside the folder to sync, \
                 nor the other way round. Choose another one.",
                "A pasta das cópias cifradas não pode ficar dentro da pasta para sincronizar, \
                 nem o contrário. Escolha outra.",
            )
            .to_string());
        }
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
    let next = if kind == StorageKind::Directory {
        t(
            "This folder syncs on its own every two minutes; the Satchel menu shows when. The \
             encrypted copies are on this computer, so no other device can join.",
            "Esta pasta sincroniza sozinha a cada dois minutos; o menu do Satchel mostra quando. \
             As cópias cifradas estão neste computador, então nenhum outro aparelho entra.",
        )
    } else {
        t(
            "Next: Add device, on the next page or in the Satchel menu, brings in your phone or \
             another computer. This folder syncs on its own every two minutes.",
            "Próximo passo: Adicionar aparelho, na próxima página ou no menu do Satchel, traz o \
             celular ou outro computador. Esta pasta sincroniza sozinha a cada dois minutos.",
        )
    };
    let base = format!("/{}/", shared.token);
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
            "<h1>{}</h1>{warning}<p>{}</p>{}<p class=\"hint\">{}</p>{}",
            t("Repository created", "Repositório criado"),
            t(
                "Save this recovery password in a password manager that opens without this \
                 repository. It is shown once and the official <code>restic</code> uses it.",
                "Guarde esta senha de recuperação num gerenciador de senhas que abra sem este \
                 repositório. Ela aparece uma vez só, e o <code>restic</code> oficial usa a mesma.",
            ),
            secret_block(language, &created.recovery_password),
            next,
            saved_button(language, &base),
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
            "<h1>{}</h1><p>{}</p><a class=\"button\" href=\"/{}/\">{}</a>",
            t("Joined", "Pronto, este aparelho entrou"),
            t(
                "This device has its own key and opened the repository with it. The first sync \
                 runs within two minutes; the Satchel menu shows when it ends. The recovery \
                 password stays with the device that created the repository.",
                "Este aparelho tem a própria chave e já abriu o repositório com ela. A primeira \
                 sincronização roda em até dois minutos; o menu do Satchel mostra quando termina. \
                 A senha de recuperação fica com o aparelho que criou o repositório.",
            ),
            shared.token,
            t("See this device", "Ver este aparelho"),
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
                "Choose a folder, or type its full path, like /Users/you/Satchel.",
                "Escolha uma pasta ou digite o caminho completo, como /Users/voce/Satchel.",
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
    if desktop.profile().is_some() && error.is_none() {
        drop(desktop);
        return dashboard(shared, language, None);
    }
    let notice = match error {
        Some(message) => format!("<p class=\"error\" role=\"alert\">{}</p>", escape(message)),
        None => String::new(),
    };
    // The refused form opens again with the message beside it; nothing is open on a first visit.
    let open = if typed.contains_key("join_folder") {
        "join"
    } else if typed.contains_key("folder") {
        "create"
    } else {
        ""
    };
    let (create_notice, join_notice) = match open {
        "join" => (String::new(), notice),
        "create" => (notice, String::new()),
        _ => (String::new(), String::new()),
    };
    let manual_open = if ["address", "code", "fingerprint"]
        .iter()
        .any(|name| typed.get(*name).is_some_and(|text| !text.is_empty()))
    {
        " open"
    } else {
        ""
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
    let has_picker = shared.picker.is_some();
    let choose = t("Choose…", "Escolher…");
    // A field for a folder: typed, or filled by the system chooser when there is one.
    let folder_field = |name: &str, value: &str, placeholder: &str, target: PickFor| {
        let input =
            format!("<input id=\"{name}\" name=\"{name}\"{value} placeholder=\"{placeholder}\">");
        if !has_picker {
            return input;
        }
        format!(
            "<div class=\"row\">{input}<button type=\"button\" class=\"pick\" \
             data-for=\"{}\" data-target=\"{name}\">{choose}</button></div>",
            target.key()
        )
    };
    page(
        language,
        &format!(
            "<h1>{title}</h1>
<p class=\"lead\">{lead}</p>
<div class=\"choice\" role=\"group\" aria-label=\"{choice_label}\">
<button type=\"button\" class=\"secondary\" data-open=\"create\" aria-controls=\"create\" aria-pressed=\"false\"><strong>{choose_create}</strong><span>{choose_create_hint}</span></button>
<button type=\"button\" class=\"secondary\" data-open=\"join\" aria-controls=\"join\" aria-pressed=\"false\"><strong>{choose_join}</strong><span>{choose_join_hint}</span></button></div>
<section class=\"card\" id=\"create\"><h2>{create_title}</h2>
<p class=\"hint\">{create_hint}</p>{create_notice}
<form method=\"post\" action=\"{base}create\" onsubmit=\"busy(this,'{checking}')\">
<label for=\"folder\">{synced}</label>{pick_folder}
<p class=\"hint\">{synced_hint}</p>
<label>{storage} <select name=\"kind\">
<option value=\"folder\">{local}</option><option value=\"s3\"{k_s3}>S3</option>
<option value=\"r2\"{k_r2}>Cloudflare R2</option><option value=\"b2\"{k_b2}>Backblaze B2</option></select></label>
<div id=\"fol\"><label for=\"repository\">{repo_folder}</label>{pick_repository}
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
<section class=\"card\" id=\"join\"><h2>{join_title}</h2>
<p class=\"hint\">{join_hint}</p>{join_notice}
<form method=\"post\" action=\"{base}join\" onsubmit=\"busy(this,'{waiting}')\">
<label for=\"join_folder\">{synced}</label>{pick_join}
<p class=\"hint\">{join_synced_hint}</p>
<label>{device_name} <input name=\"device_name\"{v_device_name} placeholder=\"Desktop\"></label>
<label>{code} <input name=\"code\"{v_code} inputmode=\"numeric\" autocomplete=\"off\"></label>
<label>{address} <input name=\"address\"{v_address} autocomplete=\"off\"></label>
<label>{fingerprint} <input name=\"fingerprint\"{v_fingerprint} autocomplete=\"off\"></label>
<p class=\"hint\">{manual_hint}</p>
<details{manual_open}><summary>{paste_summary}</summary>
<label>{paste} <textarea name=\"invite\" rows=\"3\">{invite}</textarea></label>
<p class=\"hint\">{paste_hint}</p></details>
<button>{join}</button></form></section>
<script>
const opened='{open}';
function openCard(id){{for(const c of ['create','join']){{document.getElementById(c).hidden=id!==c;}}
for(const b of document.querySelectorAll('[data-open]')){{b.setAttribute('aria-pressed',String(b.dataset.open===id));}}}}
for(const b of document.querySelectorAll('[data-open]')){{b.addEventListener('click',()=>{{openCard(b.dataset.open);document.querySelector('#'+b.dataset.open+' input').focus();}});}}
openCard(opened);
if(opened){{const e=document.querySelector('#'+opened+' .error');if(e){{e.tabIndex=-1;e.focus();}}}}
const kind=document.querySelector('select[name=kind]');
function show(){{const o=kind.value!=='folder';document.getElementById('obj').hidden=!o;document.getElementById('fol').hidden=o;}}
kind.addEventListener('change',show);show();
function busy(f,t){{const b=f.querySelector('button:not(.pick)');b.disabled=true;b.textContent=t;b.setAttribute('aria-busy','true');
for(const e of document.querySelectorAll('.error'))e.hidden=true;}}
for(const b of document.querySelectorAll('button.pick')){{b.addEventListener('click',async()=>{{
const label=b.textContent;b.disabled=true;b.textContent='{choosing}';
try{{const r=await fetch('{base}pick',{{method:'POST',headers:{{'Content-Type':'application/x-www-form-urlencoded'}},body:'for='+b.dataset.for}});
if(r.status===200){{const i=document.getElementById(b.dataset.target);i.value=await r.text();i.focus();}}}}
finally{{b.disabled=false;b.textContent=label;}}}});}}
</script>",
            choice_label = t("How to start", "Como começar"),
            choose_create = t("This is my first device", "Este é o meu primeiro aparelho"),
            choose_create_hint = t(
                "Create the repository and choose where the encrypted copies go.",
                "Criar o repositório e escolher onde ficam as cópias cifradas.",
            ),
            choose_join = t("I already use Satchel on another device", "Já uso o Satchel noutro aparelho"),
            choose_join_hint = t(
                "Join it with the code it shows. No storage account to type.",
                "Entrar com o código que ele mostra. Sem digitar a conta do armazenamento.",
            ),
            manual_hint = t(
                "The three appear on the other device, under Add device. Type them as they \
                 appear there, spaces included.",
                "Os três aparecem no outro aparelho, em Adicionar aparelho. Digite como aparecem \
                 lá, com os espaços.",
            ),
            paste_summary = t("Paste the whole invite instead", "Colar o convite inteiro"),
            paste_hint = t(
                "Copy invite, on the other computer's Add device page, copies it. With the invite \
                 the three fields above can stay empty.",
                "Copiar convite, na página Adicionar aparelho do outro computador, copia o convite. \
                 Com ele, os três campos acima podem ficar vazios.",
            ),
            pick_folder = folder_field("folder", &v_folder, folder_example, PickFor::Synced),
            pick_repository = folder_field("repository", &v_repository, "", PickFor::Repository),
            pick_join = folder_field("join_folder", &v_join_folder, folder_example, PickFor::Synced),
            choosing = t("Choosing…", "Escolhendo…"),
            title = t("Set up this device", "Configurar este aparelho"),
            lead = t(
                "Satchel keeps one folder the same on all your devices. Every version goes, \
                 encrypted, to a storage you choose, and only your devices open it.",
                "O Satchel mantém uma pasta igual em todos os seus aparelhos. Cada versão vai, \
                 cifrada, para um armazenamento que você escolhe, e só os seus aparelhos abrem.",
            ),
            create_title = t("Create the repository", "Criar o repositório"),
            create_hint = t(
                "Do this once, on one device. It keeps the recovery password and the \
                 storage account. Your other devices join with a code and never type the account.",
                "Faça isto uma vez, num aparelho só. Ele guarda a senha de recuperação e a \
                 conta do armazenamento. Os outros aparelhos entram com um código e nunca \
                 digitam a conta.",
            ),
            synced_hint = t(
                "The files in this folder show up on your other devices. It can already have files.",
                "Os arquivos desta pasta aparecem nos seus outros aparelhos. Ela pode já ter arquivos.",
            ),
            join_synced_hint = t(
                "The files from your other devices arrive in this folder.",
                "Os arquivos dos seus outros aparelhos chegam nesta pasta.",
            ),
            checking = t("Checking the storage…", "Conferindo o armazenamento…"),
            synced = t("Folder to sync", "Pasta para sincronizar"),
            storage = t("Where to keep the encrypted copies", "Onde guardar as cópias cifradas"),
            local = t("A folder on this computer", "Uma pasta neste computador"),
            repo_folder = t("Folder for the encrypted copies", "Pasta das cópias cifradas"),
            local_hint = t(
                "Pick an empty folder outside the folder to sync, ideally on another disk. \
                 Phones and other computers can't reach a folder on this computer, so Add \
                 device stays off: to use Satchel on a phone, choose S3, R2 or B2. A folder also has no object versioning, so an \
                 overwritten copy can't be brought back.",
                "Escolha uma pasta vazia fora da pasta para sincronizar, de preferência noutro \
                 disco. Celulares e outros computadores não alcançam uma pasta deste computador, \
                 então Adicionar aparelho fica desligado: para usar o Satchel no celular, \
                 escolha S3, R2 ou B2. Uma pasta também não tem versionamento de \
                 objetos, então uma cópia sobrescrita não volta.",
            ),
            bucket_hint = t(
                "On S3 and B2 the bucket needs versioning on and no lifecycle rule that deletes \
                 objects. R2 has no versioning: Satchel accepts it and warns that an overwritten \
                 copy can't come back, and still refuses a rule that deletes objects. The \
                 maintenance key manages the bucket (on R2, an API token with Admin Read & \
                 Write). The device key is a second one, limited to listing, reading and \
                 writing; it can only delete its own locks where the store applies per-key \
                 policy (AWS S3, MinIO, RustFS). On R2 and B2 it can delete everything. On S3 (when \
                 the maintenance key may manage IAM users) and on B2 (when it has writeKeys), each \
                 device gets its own key instead, and removing a device deletes it.",
                "No S3 e no B2, o bucket precisa de versionamento ligado e de nenhuma regra de \
                 ciclo de vida que apague objetos. O R2 não tem versionamento: o Satchel aceita, \
                 avisa que uma cópia sobrescrita não volta, e continua recusando uma regra que \
                 apague objetos. A chave de manutenção administra o bucket (no R2, um token de \
                 API com Admin Read & Write). A chave dos aparelhos é outra, só para listar, ler \
                 e gravar; ela só fica limitada a apagar os próprios locks quando o armazenamento \
                 aplica política por chave (AWS S3, MinIO, RustFS). No R2 e no B2 ela pode \
                 apagar tudo. No S3 (quando a chave de manutenção pode administrar usuários \
                 do IAM) e no B2 (quando ela tem writeKeys), cada aparelho recebe a própria chave, \
                 e remover um aparelho a apaga.",
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
                "On a device that already uses Satchel, open Add device. Keep it open: it shows \
                 the code here and asks you to approve this device.",
                "Num aparelho que já usa o Satchel, abra Adicionar aparelho. Deixe aberto: ele \
                 mostra o código e pede para aprovar este aparelho.",
            ),
            waiting = t(
                "Waiting for the other device to approve…",
                "Esperando o outro aparelho aprovar…",
            ),
            device_name = t("Device name", "Nome do aparelho"),
            address = t("Address", "Endereço"),
            code = t("Code", "Código"),
            fingerprint = t("Fingerprint", "Impressão digital"),
            paste = t("Invite", "Convite"),
            join = t("Join", "Entrar"),
        ),
    )
}

/// The recovery password, once, with a copy button. Monospace and wrapping.
/// The password in groups of four to read it back; Copy copies it without the spaces.
fn secret_block(language: Language, secret: &str) -> String {
    format!(
        "<div class=\"secret\"><code id=\"secret\" data-secret=\"{raw}\">{grouped}</code>\
         <button type=\"button\" class=\"quiet\" onclick=\"navigator.clipboard.writeText(\
         document.getElementById('secret').dataset.secret).then(()=>{{toast('{copied}','ok')}})\">\
         {copy}</button><p class=\"hint\">{spaces}</p></div>",
        raw = escape(secret),
        grouped = escape(&grouped_fingerprint(secret)),
        copied = language.pick("Copied", "Copiada"),
        copy = language.pick("Copy", "Copiar"),
        spaces = language.pick(
            "The spaces are only for reading. The password has none.",
            "Os espaços são só para ler. A senha não tem espaços.",
        ),
    )
}

/// After a recovery password is shown: the person says it is saved before moving on.
fn saved_button(language: Language, base: &str) -> String {
    format!(
        "<a class=\"button\" href=\"{base}\">{}</a>",
        language.pick("I saved it", "Guardei a senha")
    )
}

/// The page frame: the palette in docs/identidade-visual.md, light or dark as the system is.
fn page(language: Language, body: &str) -> String {
    format!(
        "<!doctype html><html lang=\"{}\"><head><meta charset=\"utf-8\">\
<meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\
<meta name=\"color-scheme\" content=\"light dark\"><title>Satchel</title><style>\
:root{{--canvas:#FBFCFD;--card:#FFFFFF;--ink:#272735;--secondary:#666673;--accent:#4B5FE0;\
--field:#F0F1F5;--line:#DDDDE5;--danger:#C02868;--success:#187A4E;--warning:#B34900}}\
@media (prefers-color-scheme:dark){{:root{{--canvas:#10101A;--card:#1C1C28;--ink:#EDEDF3;\
--secondary:#A3A3AE;--accent:#9CB4E8;--field:#2A2A38;--line:#363644;--danger:#FC92B4;\
--success:#77C599;--warning:#FC9B6F}}}}\
*{{box-sizing:border-box}}\
body{{margin:0;background:var(--canvas);color:var(--ink);font:1rem/1.5 system-ui,-apple-system,sans-serif}}\
main{{max-width:34rem;margin:0 auto;padding:1.5rem 1.25rem 3rem}}\
.brand{{display:flex;align-items:center;gap:.75rem;font-weight:600}}\
.brand img{{width:36px;height:36px;border-radius:8px}}\
h1{{font-size:2rem;font-weight:600;margin:1rem 0 .5rem;letter-spacing:-.01em}}\
h2{{font-size:1.06rem;font-weight:600;margin:0 0 .25rem}}\
p{{margin:.5rem 0}}.lead,.hint{{color:var(--secondary)}}.hint{{font-size:.94rem}}\
.card{{background:var(--card);border:1px solid var(--line);border-radius:12px;padding:1rem;margin:.75rem 0}}\
label{{display:block;margin:.75rem 0 0;font-size:.82rem;font-weight:600;color:var(--secondary)}}\
input,select,textarea{{display:block;width:100%;margin-top:.3rem;padding:.7rem .75rem;border:0;\
border-radius:8px;background:var(--field);color:var(--ink);font:400 1rem/1.4 system-ui,-apple-system,sans-serif}}\
input:focus,select:focus,textarea:focus,button:focus-visible{{outline:2px solid var(--accent);outline-offset:2px}}\
button{{display:block;width:100%;min-height:48px;margin-top:1rem;border:0;border-radius:10px;\
background:var(--accent);color:var(--card);font:inherit;font-weight:600;cursor:pointer}}\
button:disabled{{opacity:.6;cursor:progress}}a{{color:var(--accent)}}a.button{{display:block;min-height:48px;margin-top:1rem;padding:.75rem 1rem;border-radius:10px;background:var(--accent);color:var(--card);font-weight:600;text-align:center;text-decoration:none}}a.button.secondary{{background:var(--field);color:var(--ink);border:1px solid var(--line)}}a.button:focus-visible{{outline:2px solid var(--accent);outline-offset:2px}}.state{{font-size:1.06rem;font-weight:600}}.choice{{display:grid;gap:.75rem;margin:1rem 0}}.choice button{{margin:0;padding:.9rem 1rem;text-align:left;display:flex;flex-direction:column;gap:.2rem}}.choice button span{{font-weight:400;color:var(--secondary);font-size:.94rem}}.choice button[aria-pressed=true]{{outline:2px solid var(--accent);outline-offset:-2px}}details{{margin-top:.75rem}}summary{{cursor:pointer;color:var(--accent);font-weight:600}}\
.row{{display:flex;gap:.5rem;align-items:center}}.row input{{flex:1;min-width:0}}\
button.pick{{width:auto;flex:none;min-height:44px;margin-top:.3rem;padding:0 1rem;background:var(--field);\
color:var(--ink);border:1px solid var(--line)}}\
.toasts{{position:fixed;right:1rem;bottom:1rem;left:1rem;display:flex;flex-direction:column;align-items:flex-end;gap:.5rem;pointer-events:none;z-index:10}}\
.toast{{pointer-events:auto;display:flex;gap:.75rem;align-items:flex-start;max-width:24rem;padding:.75rem 1rem;border-radius:10px;\
background:var(--ink);color:var(--canvas);box-shadow:0 8px 24px rgba(0,0,0,.2);opacity:0;transform:translateY(.5rem);\
transition:opacity .2s,transform .2s;font-size:.94rem}}.toast.in{{opacity:1;transform:none}}.toast.bad{{background:var(--danger);color:var(--card)}}\
.toast .close{{display:inline;width:auto;min-height:0;margin:0 0 0 auto;padding:0 .25rem;background:none;color:inherit;font-size:1.1rem;line-height:1}}\
@media (prefers-reduced-motion:reduce){{.toast{{transition:none}}}}\
.badge{{display:inline-block;margin-left:.35rem;padding:.05rem .5rem;border-radius:999px;background:var(--accent);color:var(--card);font-size:.75rem;font-weight:600;vertical-align:middle}}\
.badge.quiet{{background:var(--field);color:var(--secondary)}}\
button.danger{{background:var(--danger);color:var(--card)}}\
.folder{{border-top:1px solid var(--line);padding:.75rem 0 0;margin-top:.75rem}}.folder:first-of-type{{border-top:0;margin-top:0}}\
h3{{font-size:1rem;font-weight:600;margin:0}}.done{{color:var(--accent);font-weight:600}}\
label.check{{display:flex;gap:.5rem;align-items:flex-start;font-weight:400;color:var(--ink);font-size:.94rem}}\
label.check input{{width:auto;margin:.2rem 0 0}}\
button.secondary{{background:var(--field);color:var(--ink);border:1px solid var(--line)}}\
img.qr{{display:block;width:min(100%,18rem);aspect-ratio:1;margin:1rem auto;image-rendering:pixelated;\
background:#fff;border-radius:12px;padding:.75rem}}\
.code-inline{{font:600 1.1rem/1.2 ui-monospace,SFMono-Regular,Menlo,monospace;letter-spacing:.06em}}.code{{font:600 2.25rem/1.2 ui-monospace,SFMono-Regular,Menlo,monospace;letter-spacing:.08em;margin:.25rem 0 .75rem}}\
button.quiet{{display:inline;width:auto;min-height:0;margin:0;padding:.25rem 0;background:none;color:var(--accent)}}\
.error{{color:var(--danger);font-weight:600}}.warn{{color:var(--warning);font-weight:600}}.dot{{flex:none;display:inline-block;width:.6rem;height:.6rem;margin-right:.5rem;border-radius:50%;background:var(--secondary)}}.dot.ok{{background:var(--success)}}.dot.bad{{background:var(--danger)}}code,.code,.code-inline{{font-variant-numeric:tabular-nums}}\
.secret{{background:var(--card);border:1px solid var(--line);border-radius:12px;padding:1rem;margin:.75rem 0}}\
.secret code{{display:block;font:1.05rem/1.6 ui-monospace,SFMono-Regular,Menlo,monospace;overflow-wrap:anywhere}}\
code{{font-family:ui-monospace,SFMono-Regular,Menlo,monospace}}\
</style></head><body><main><div class=\"brand\"><img src=\"mark.png\" alt=\"\">Satchel</div>\
{body}</main><div class=\"toasts\" id=\"toasts\"></div><script>\
function toast(text,kind){{const box=document.getElementById('toasts');const t=document.createElement('div');\
t.className='toast'+(kind==='error'?' bad':'');t.setAttribute('role',kind==='error'?'alert':'status');\
t.textContent=text;const x=document.createElement('button');x.type='button';x.className='close';\
x.setAttribute('aria-label','{close}');x.textContent='×';x.onclick=()=>t.remove();t.appendChild(x);box.appendChild(t);\
requestAnimationFrame(()=>t.classList.add('in'));\
if(kind!=='error'){{setTimeout(()=>{{t.classList.remove('in');setTimeout(()=>t.remove(),250);}},4500);}}}}\
for(const e of document.querySelectorAll('[data-toast]')){{toast(e.textContent,e.dataset.toast);\
if(e.dataset.toast!=='error'){{e.hidden=true;}}}}\
</script></body></html>",
        language.html_tag(),
        close = language.pick("Close", "Fechar"),
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
        204 => "No Content",
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
