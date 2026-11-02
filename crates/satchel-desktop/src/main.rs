#[cfg(target_os = "macos")]
use std::collections::BTreeMap;
#[cfg(target_os = "macos")]
use std::ffi::c_void;
use std::net::SocketAddr;
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
#[cfg(target_os = "linux")]
use std::thread;
use std::time::{Duration, Instant};

#[cfg(target_os = "macos")]
use satchel_desktop::TrayView;
use satchel_desktop::{
    Desktop, KeyringStore, SetupPage, TrayCommand, default_config_dir, start_setup_page,
};
use satchel_pair::{DEFAULT_TIMEOUT, ListenConfig};

#[cfg(target_os = "linux")]
use ksni::Tray;
#[cfg(target_os = "linux")]
use ksni::blocking::TrayMethods;
#[cfg(target_os = "linux")]
use ksni::menu::{MenuItem, StandardItem};

fn main() -> ExitCode {
    if !display_available() {
        eprintln!("satchel-desktop: no display, tray not started");
        return ExitCode::from(2);
    }
    if let Err(err) = run() {
        eprintln!("satchel-desktop: {err}");
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}

fn display_available() -> bool {
    #[cfg(target_os = "macos")]
    {
        active_display_count() > 0
    }
    #[cfg(not(target_os = "macos"))]
    {
        std::env::var_os("DISPLAY").is_some() || std::env::var_os("WAYLAND_DISPLAY").is_some()
    }
}

fn run() -> Result<(), satchel_desktop::Error> {
    let desktop = Arc::new(Mutex::new(Desktop::open(default_config_dir())?));
    let secrets = Arc::new(keyring_from_env());
    let quit = Arc::new(AtomicBool::new(false));
    #[cfg(target_os = "linux")]
    {
        linux_tray(desktop, secrets, quit)
    }
    #[cfg(target_os = "macos")]
    {
        macos_tray(desktop, secrets, quit)
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = (desktop, secrets, quit);
        Err(satchel_desktop::Error::Tray)
    }
}

#[cfg(target_os = "linux")]
fn linux_tray(
    desktop: Arc<Mutex<Desktop>>,
    secrets: Arc<KeyringStore>,
    quit: Arc<AtomicBool>,
) -> Result<(), satchel_desktop::Error> {
    let tray_desktop = Arc::clone(&desktop);
    let tray_secrets = Arc::clone(&secrets);
    let tray = SatchelTray {
        desktop,
        secrets,
        quit: Arc::clone(&quit),
    };
    let handle = tray.spawn().map_err(|_| satchel_desktop::Error::Tray)?;
    let mut auto = AutoSync::new();
    while !quit.load(Ordering::SeqCst) {
        thread::sleep(Duration::from_millis(200));
        auto.tick(&tray_desktop, &tray_secrets);
    }
    drop(handle);
    Ok(())
}

#[cfg(target_os = "linux")]
struct SatchelTray {
    desktop: Arc<Mutex<Desktop>>,
    secrets: Arc<KeyringStore>,
    quit: Arc<AtomicBool>,
}

#[cfg(target_os = "linux")]
impl Tray for SatchelTray {
    fn id(&self) -> String {
        "satchel".to_string()
    }

    fn title(&self) -> String {
        "Satchel".to_string()
    }

    fn icon_name(&self) -> String {
        "folder-remote".to_string()
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let view = {
            let mut desktop = lock_desktop(&self.desktop);
            desktop.poll();
            desktop.tray_view()
        };
        let mut items = Vec::new();
        for item in view.items {
            let command = item.command;
            let icon_data = item.icon_png.unwrap_or_default();
            items.push(MenuItem::Standard(StandardItem {
                label: item.label,
                enabled: item.enabled,
                icon_data,
                activate: Box::new(move |tray: &mut Self| {
                    apply_command(&tray.desktop, &tray.secrets, &tray.quit, command.clone());
                }),
                ..Default::default()
            }));
        }
        items
    }
}

#[cfg(target_os = "macos")]
fn macos_tray(
    desktop: Arc<Mutex<Desktop>>,
    secrets: Arc<KeyringStore>,
    quit: Arc<AtomicBool>,
) -> Result<(), satchel_desktop::Error> {
    use tray_icon::TrayIconBuilder;
    use tray_icon::menu::{Menu, MenuEvent};

    let icon = status_icon()?;
    let menu = Menu::new();
    let tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("Satchel")
        .with_icon(icon)
        .with_icon_as_template(true)
        .with_menu_on_left_click(true)
        .build()
        .map_err(|_| satchel_desktop::Error::Tray)?;
    let mut shown: Option<TrayView> = None;
    let mut commands: BTreeMap<String, TrayCommand> = BTreeMap::new();
    let events = MenuEvent::receiver();
    let mut auto = AutoSync::new();
    while !quit.load(Ordering::SeqCst) {
        auto.tick(&desktop, &secrets);
        let view = {
            let mut opened = lock_desktop(&desktop);
            opened.poll();
            opened.tray_view()
        };
        if shown.as_ref() != Some(&view) {
            let built = menu_for(&view)?;
            commands = built.commands;
            tray.set_menu(Some(Box::new(built.menu)));
            shown = Some(view);
        }
        pump_run_loop();
        while let Ok(event) = events.try_recv() {
            let id = event.id.as_ref();
            let Some(command) = commands.get(id).cloned() else {
                continue;
            };
            apply_command(&desktop, &secrets, &quit, command);
        }
    }
    drop(tray);
    Ok(())
}

#[cfg(target_os = "macos")]
struct BuiltMenu {
    menu: tray_icon::menu::Menu,
    commands: BTreeMap<String, TrayCommand>,
}

#[cfg(target_os = "macos")]
fn menu_for(view: &TrayView) -> Result<BuiltMenu, satchel_desktop::Error> {
    use tray_icon::menu::{IconMenuItem, Menu, MenuItem};

    let menu = Menu::new();
    let mut commands = BTreeMap::new();
    enum Row {
        Text(MenuItem),
        Icon(IconMenuItem),
    }
    let mut rows = Vec::new();
    for (index, item) in view.items.iter().enumerate() {
        let id = format!("row-{index}");
        commands.insert(id.clone(), item.command.clone());
        if let Some(png) = item.icon_png.as_ref()
            && let Some(icon) = menu_icon(png)
        {
            rows.push(Row::Icon(IconMenuItem::with_id(
                id,
                &item.label,
                item.enabled,
                Some(icon),
                None,
            )));
            continue;
        }
        rows.push(Row::Text(MenuItem::with_id(
            id,
            &item.label,
            item.enabled,
            None,
        )));
    }
    let mut refs = Vec::new();
    for row in &rows {
        match row {
            Row::Text(item) => refs.push(item as &dyn tray_icon::menu::IsMenuItem),
            Row::Icon(item) => refs.push(item as &dyn tray_icon::menu::IsMenuItem),
        }
    }
    menu.append_items(&refs)
        .map_err(|_| satchel_desktop::Error::Tray)?;
    Ok(BuiltMenu { menu, commands })
}

#[cfg(target_os = "macos")]
fn menu_icon(png: &[u8]) -> Option<tray_icon::menu::Icon> {
    let decoded = image::load_from_memory(png).ok()?;
    let rgba = decoded.to_rgba8();
    let (width, height) = rgba.dimensions();
    if width == 0 || height == 0 {
        return None;
    }
    tray_icon::menu::Icon::from_rgba(rgba.into_raw(), width, height).ok()
}

#[cfg(target_os = "macos")]
fn tray_mark_rgba() -> &'static [u8] {
    satchel_desktop::tray_mark_rgba()
}

fn keyring_from_env() -> KeyringStore {
    let Ok(service) = std::env::var("SATCHEL_KEYCHAIN_SERVICE") else {
        return KeyringStore::system();
    };
    if service.is_empty() {
        return KeyringStore::system();
    }
    KeyringStore::scoped(service)
}

#[cfg(target_os = "macos")]
fn status_icon() -> Result<tray_icon::Icon, satchel_desktop::Error> {
    const SIDE: u32 = 36;
    let rgba = tray_mark_rgba();
    if rgba.len() != (SIDE * SIDE * 4) as usize {
        return Err(satchel_desktop::Error::Tray);
    }
    tray_icon::Icon::from_rgba(rgba.to_vec(), SIDE, SIDE).map_err(|_| satchel_desktop::Error::Tray)
}

#[cfg(target_os = "macos")]
fn pump_run_loop() {
    unsafe {
        CFRunLoopRunInMode(kCFRunLoopDefaultMode, 0.2, 0);
    }
}

#[cfg(target_os = "macos")]
fn active_display_count() -> u32 {
    let mut count = 0u32;
    let status = unsafe { CGGetActiveDisplayList(0, std::ptr::null_mut(), &mut count) };
    if status == 0 { count } else { 0 }
}

#[cfg(target_os = "macos")]
#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRunLoopRunInMode(
        mode: *const c_void,
        seconds: f64,
        return_after_source_handled: u8,
    ) -> i32;
    static kCFRunLoopDefaultMode: *const c_void;
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

/// Both roles sync on their own once a folder is set up. A pairing screen pauses it.
struct AutoSync {
    last: Instant,
}

const AUTO_SYNC_EVERY: Duration = Duration::from_secs(120);

impl AutoSync {
    fn new() -> Self {
        Self {
            last: Instant::now(),
        }
    }

    fn tick(&mut self, desktop: &Mutex<Desktop>, secrets: &KeyringStore) {
        if self.last.elapsed() < AUTO_SYNC_EVERY {
            return;
        }
        self.last = Instant::now();
        let mut desktop = lock_desktop(desktop);
        if desktop.profile().is_none() || desktop.add_device_open() {
            return;
        }
        if let Err(err) = desktop.publish(secrets) {
            eprintln!("satchel-desktop: {err}");
        }
    }
}

static SETUP: OnceLock<SetupPage> = OnceLock::new();

fn open_setup(desktop: &Arc<Mutex<Desktop>>, secrets: &Arc<KeyringStore>) {
    if SETUP.get().is_none() {
        match start_setup_page(Arc::clone(desktop), Arc::clone(secrets)) {
            Ok(page) => {
                let _ = SETUP.set(page);
            }
            Err(_) => {
                eprintln!("satchel-desktop: the setup page did not start");
                return;
            }
        }
    }
    let Some(page) = SETUP.get() else {
        return;
    };
    let opener = if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    if std::process::Command::new(opener)
        .arg(page.url())
        .spawn()
        .is_err()
    {
        eprintln!("satchel-desktop: could not open a browser");
    }
}

fn apply_command(
    desktop: &Arc<Mutex<Desktop>>,
    secrets: &Arc<KeyringStore>,
    quit: &AtomicBool,
    command: TrayCommand,
) {
    match command {
        TrayCommand::OpenSetup => open_setup(desktop, secrets),
        TrayCommand::None => {}
        TrayCommand::Quit => quit.store(true, Ordering::SeqCst),
        TrayCommand::OpenAddDevice => {
            let mut desktop = lock_desktop(desktop);
            // An unspecified address in the invite cannot be dialed from another device.
            let ip = satchel_pair::preferred_bind_ip().unwrap_or_else(|| [0, 0, 0, 0].into());
            let config = ListenConfig {
                bind: SocketAddr::new(ip, 0),
                timeout: DEFAULT_TIMEOUT,
                advertise: true,
            };
            if let Err(err) = desktop.open_add_device(config) {
                eprintln!("satchel-desktop: {err}");
            }
        }
        TrayCommand::CloseAddDevice => {
            lock_desktop(desktop).close_add_device();
        }
        TrayCommand::Approve => {
            let mut desktop = lock_desktop(desktop);
            if let Err(err) = desktop.approve_pending(&**secrets) {
                eprintln!("satchel-desktop: {err}");
            }
        }
        TrayCommand::Deny => {
            let mut desktop = lock_desktop(desktop);
            if let Err(err) = desktop.deny_pending() {
                eprintln!("satchel-desktop: {err}");
            }
        }
        TrayCommand::Publish => {
            let mut desktop = lock_desktop(desktop);
            match desktop.publish(&**secrets) {
                Ok(report) => {
                    if report.protection == satchel_core::SetupCheck::Unprotected {
                        eprintln!("satchel-desktop: this folder has no object versioning");
                    }
                }
                Err(err) => eprintln!("satchel-desktop: {err}"),
            }
        }
        TrayCommand::LoadDevices => {
            let mut desktop = lock_desktop(desktop);
            if let Err(err) = desktop.load_device_keys(&**secrets) {
                eprintln!("satchel-desktop: {err}");
            }
        }
        TrayCommand::Revoke(key_id) => {
            let mut desktop = lock_desktop(desktop);
            if let Err(err) = desktop.revoke_device_key(&**secrets, &key_id) {
                eprintln!("satchel-desktop: {err}");
            }
        }
    }
}

fn lock_desktop(desktop: &Mutex<Desktop>) -> std::sync::MutexGuard<'_, Desktop> {
    match desktop.lock() {
        Ok(guard) => guard,
        Err(poison) => poison.into_inner(),
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tray_mark_tests {
    #[test]
    fn menu_bar_mark_is_a_36_pixel_template() {
        let bytes = super::tray_mark_rgba();
        assert_eq!(bytes.len(), 36 * 36 * 4);
        let mut index = 0;
        while index < bytes.len() {
            assert_eq!(bytes[index], 0);
            assert_eq!(bytes[index + 1], 0);
            assert_eq!(bytes[index + 2], 0);
            index += 4;
        }
        let mut opaque = 0;
        let mut pixel = 3;
        while pixel < bytes.len() {
            if bytes[pixel] > 16 {
                opaque += 1;
            }
            pixel += 4;
        }
        assert!(opaque > 0);
    }
}
