use std::net::SocketAddr;
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use ksni::Tray;
use ksni::blocking::TrayMethods;
use ksni::menu::{MenuItem, StandardItem};
use satchel_desktop::{Desktop, KeyringStore, TrayCommand, default_config_dir};
use satchel_pair::{DEFAULT_TIMEOUT, ListenConfig};

fn main() -> ExitCode {
    if std::env::var_os("DISPLAY").is_none() && std::env::var_os("WAYLAND_DISPLAY").is_none() {
        eprintln!("satchel-desktop: no display, tray not started");
        return ExitCode::from(2);
    }
    if let Err(err) = run() {
        eprintln!("satchel-desktop: {err}");
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}

fn run() -> Result<(), satchel_desktop::Error> {
    let desktop = Desktop::open(default_config_dir())?;
    let quit = Arc::new(AtomicBool::new(false));
    let tray = SatchelTray {
        desktop: Arc::new(Mutex::new(desktop)),
        secrets: Arc::new(KeyringStore::system()),
        quit: Arc::clone(&quit),
    };
    let handle = tray.spawn().map_err(|_| satchel_desktop::Error::Tray)?;
    while !quit.load(Ordering::SeqCst) {
        thread::sleep(Duration::from_millis(200));
    }
    drop(handle);
    Ok(())
}

struct SatchelTray {
    desktop: Arc<Mutex<Desktop>>,
    secrets: Arc<KeyringStore>,
    quit: Arc<AtomicBool>,
}

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
                    tray.run_command(command.clone());
                }),
                ..Default::default()
            }));
        }
        items
    }
}

impl SatchelTray {
    fn run_command(&mut self, command: TrayCommand) {
        match command {
            TrayCommand::None => {}
            TrayCommand::Quit => self.quit.store(true, Ordering::SeqCst),
            TrayCommand::OpenAddDevice => {
                let mut desktop = lock_desktop(&self.desktop);
                let config = ListenConfig {
                    bind: SocketAddr::from(([0, 0, 0, 0], 0)),
                    timeout: DEFAULT_TIMEOUT,
                    advertise: true,
                };
                if let Err(err) = desktop.open_add_device(config) {
                    eprintln!("satchel-desktop: {err}");
                }
            }
            TrayCommand::CloseAddDevice => {
                lock_desktop(&self.desktop).close_add_device();
            }
            TrayCommand::Approve => {
                let mut desktop = lock_desktop(&self.desktop);
                if let Err(err) = desktop.approve_pending(self.secrets.as_ref()) {
                    eprintln!("satchel-desktop: {err}");
                }
            }
            TrayCommand::Deny => {
                let mut desktop = lock_desktop(&self.desktop);
                if let Err(err) = desktop.deny_pending() {
                    eprintln!("satchel-desktop: {err}");
                }
            }
            TrayCommand::Publish => {
                let mut desktop = lock_desktop(&self.desktop);
                match desktop.publish(self.secrets.as_ref()) {
                    Ok(report) => {
                        if report.protection == satchel_core::SetupCheck::Unprotected {
                            eprintln!("satchel-desktop: this folder has no object versioning");
                        }
                    }
                    Err(err) => eprintln!("satchel-desktop: {err}"),
                }
            }
            TrayCommand::LoadDevices => {
                let mut desktop = lock_desktop(&self.desktop);
                if let Err(err) = desktop.load_device_keys(self.secrets.as_ref()) {
                    eprintln!("satchel-desktop: {err}");
                }
            }
            TrayCommand::Revoke(key_id) => {
                let mut desktop = lock_desktop(&self.desktop);
                if let Err(err) = desktop.revoke_device_key(self.secrets.as_ref(), &key_id) {
                    eprintln!("satchel-desktop: {err}");
                }
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
