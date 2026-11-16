//! Serve the setup page from a throwaway config and an in-memory keystore, to look at it.
//!
//! `cargo run -p satchel-desktop --example setup_preview -- <empty-dir> <seconds>` prints the URL.
//! Nothing touches the real keystore or the real config.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use satchel_desktop::{Desktop, MemoryStore, start_setup_page};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let dir = args
        .first()
        .expect("an empty directory for the throwaway config");
    let seconds = args
        .get(1)
        .and_then(|text| text.parse().ok())
        .unwrap_or(300);
    let desktop = Arc::new(Mutex::new(Desktop::open(dir).expect("config")));
    let page = start_setup_page(desktop, Arc::new(MemoryStore::default())).expect("page");
    println!("{}", page.url());
    std::thread::sleep(Duration::from_secs(seconds));
}
