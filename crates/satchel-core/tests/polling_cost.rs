//! How many storage requests one sync cycle makes when nothing changed.
//!
//! A counting proxy sits between `sync_folder` and Floci and records every
//! request line. This is a measurement for the polling discussion in
//! `docs/progresso.md`, not a budget the code must meet.

#[path = "support/floci.rs"]
mod floci;

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Instant;

use floci::*;
use satchel_core::{DeviceId, Location, S3Store, quiet_mark, sync_folder};

type Log = Arc<Mutex<Vec<String>>>;

/// Forward 127.0.0.1:<port> to `upstream`, recording each HTTP request line.
fn counting_proxy(upstream: String, log: Log) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    thread::spawn(move || {
        for client in listener.incoming() {
            let Ok(client) = client else { continue };
            let Ok(server) = TcpStream::connect(&upstream) else {
                continue;
            };
            let (mut client_read, mut server_write) = (
                client.try_clone().expect("c"),
                server.try_clone().expect("s"),
            );
            let (mut server_read, mut client_write) = (server, client);
            let log = Arc::clone(&log);
            thread::spawn(move || {
                let mut window: Vec<u8> = Vec::new();
                let mut chunk = [0_u8; 16384];
                while let Ok(read) = client_read.read(&mut chunk) {
                    if read == 0 {
                        break;
                    }
                    window.extend_from_slice(&chunk[..read]);
                    record(&mut window, &log);
                    if server_write.write_all(&chunk[..read]).is_err() {
                        break;
                    }
                }
                let _ = server_write.shutdown(std::net::Shutdown::Write);
            });
            thread::spawn(move || {
                let _ = std::io::copy(&mut server_read, &mut client_write);
                let _ = client_write.shutdown(std::net::Shutdown::Write);
            });
        }
    });
    port
}

fn record(window: &mut Vec<u8>, log: &Log) {
    let text = String::from_utf8_lossy(window).into_owned();
    let mut consumed = 0;
    for method in ["GET ", "PUT ", "HEAD ", "DELETE ", "POST "] {
        let mut from = 0;
        while let Some(at) = text[from..].find(method) {
            let start = from + at;
            let line_start = start == 0 || text[..start].ends_with('\n');
            let Some(end) = text[start..].find(" HTTP/1.1\r\n") else {
                break;
            };
            if line_start {
                log.lock()
                    .expect("log")
                    .push(text[start..start + end].to_string());
            }
            from = start + end;
            consumed = consumed.max(from);
        }
    }
    // Keep only a tail so a request line split across reads is still seen.
    let keep = window.len().saturating_sub(consumed).min(4096);
    let tail = window.split_off(window.len() - keep);
    *window = tail;
}

fn classify(line: &str) -> String {
    let (method, target) = line.split_once(' ').unwrap_or((line, ""));
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let mut parts = path.trim_start_matches('/').splitn(4, '/');
    let _bucket = parts.next();
    let _prefix = parts.next();
    let kind = parts.next().unwrap_or("");
    if kind.is_empty() {
        let listed = query
            .split('&')
            .find_map(|pair| pair.strip_prefix("prefix="))
            .unwrap_or("")
            .replace("%2F", "/");
        format!("LIST {}", listed.trim_start_matches("repo/"))
    } else {
        format!("{method} {kind}")
    }
}

fn cycle(
    location: &Location,
    device: &DeviceId,
    folder: &std::path::Path,
    log: &Log,
) -> (BTreeMap<String, usize>, u128) {
    log.lock().expect("log").clear();
    let started = Instant::now();
    sync_folder(location, "polling-password", device, folder).expect("sync");
    let elapsed = started.elapsed().as_millis();
    let mut counts = BTreeMap::new();
    for line in log.lock().expect("log").iter() {
        *counts.entry(classify(line)).or_insert(0) += 1;
    }
    (counts, elapsed)
}

#[test]
fn an_idle_sync_cycle_is_measured() {
    let floci = LocalFloci::start();
    let endpoint = floci.endpoint().to_string();
    let bucket = format!("satchel-poll-{}", std::process::id());
    aws(
        &endpoint,
        &["s3api", "create-bucket", "--bucket", bucket.as_str()],
    );
    let log: Log = Arc::new(Mutex::new(Vec::new()));
    let port = counting_proxy(
        endpoint.trim_start_matches("http://").to_string(),
        Arc::clone(&log),
    );
    let location = Location::S3(
        S3Store::new(
            &bucket,
            format!("http://127.0.0.1:{port}"),
            "us-east-1",
            "repo",
            ACCESS_KEY,
            SECRET_KEY,
        )
        .expect("store"),
    );
    let device = DeviceId::generate();
    let root = tempfile::tempdir().expect("temp");
    let folder = root.path().join("folder");
    std::fs::create_dir_all(&folder).expect("folder");
    std::fs::write(folder.join("notes.txt"), b"one file").expect("file");

    let mut report =
        String::from("polling cost per sync_folder cycle (Floci, one device, one file):\n");
    let mut idle_totals = Vec::new();
    for label in ["first (creates)", "idle 1", "idle 2", "idle 3"] {
        let (counts, millis) = cycle(&location, &device, &folder, &log);
        let total: usize = counts.values().sum();
        if label.starts_with("idle") {
            idle_totals.push(total);
        }
        report.push_str(&format!("  {label}: {total} requests, {millis} ms\n"));
        for (what, count) in &counts {
            report.push_str(&format!("    {count:>3}  {what}\n"));
        }
    }
    // What the timer pays when nothing changed: one mark instead of a sync.
    let before = quiet_mark(&location, &folder).expect("mark");
    log.lock().expect("log").clear();
    let started = Instant::now();
    let after = quiet_mark(&location, &folder).expect("mark");
    let millis = started.elapsed().as_millis();
    let quiet_lines = log.lock().expect("log").clone();
    assert_eq!(before, after, "an idle repository gives the same mark");
    report.push_str(&format!(
        "  quiet check: {} requests, {millis} ms\n",
        quiet_lines.len()
    ));
    for line in &quiet_lines {
        report.push_str(&format!("    {}\n", classify(line)));
    }
    emit_proof(&report);
    assert!(idle_totals.iter().all(|total| *total > 0));
    assert_eq!(quiet_lines.len(), 1, "the quiet check is one listing");
}

/// A round reads tree packs, not file contents. With 40 files, an idle full
/// round and a round with one local change both stay at a handful of data
/// reads; comparing bytes would read every file each time.
#[test]
fn a_round_reads_no_unchanged_file_content() {
    let floci = LocalFloci::start();
    let endpoint = floci.endpoint().to_string();
    let bucket = format!("satchel-large-{}", std::process::id());
    aws(
        &endpoint,
        &["s3api", "create-bucket", "--bucket", bucket.as_str()],
    );
    let log: Log = Arc::new(Mutex::new(Vec::new()));
    let port = counting_proxy(
        endpoint.trim_start_matches("http://").to_string(),
        Arc::clone(&log),
    );
    let location = Location::S3(
        S3Store::new(
            &bucket,
            format!("http://127.0.0.1:{port}"),
            "us-east-1",
            "repo",
            ACCESS_KEY,
            SECRET_KEY,
        )
        .expect("store"),
    );
    let device = DeviceId::generate();
    let root = tempfile::tempdir().expect("temp");
    let folder = root.path().join("folder");
    std::fs::create_dir_all(&folder).expect("folder");
    let mut seed = 0x2545_F491_4F6C_DD1D_u64;
    for index in 0..40 {
        let mut bytes = Vec::with_capacity(96 * 1024);
        while bytes.len() < 96 * 1024 {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            bytes.extend_from_slice(&seed.to_le_bytes());
        }
        std::fs::write(folder.join(format!("file-{index:02}.bin")), bytes).expect("file");
    }
    cycle(&location, &device, &folder, &log);
    let (idle, _) = cycle(&location, &device, &folder, &log);
    std::fs::write(folder.join("file-07.bin"), b"now short").expect("change");
    let (changed, _) = cycle(&location, &device, &folder, &log);
    let data_reads = |counts: &BTreeMap<String, usize>| *counts.get("GET data").unwrap_or(&0);
    emit_proof(&format!(
        "40 files: idle round GET data {}, one-change round GET data {}",
        data_reads(&idle),
        data_reads(&changed)
    ));
    // Tree packs only: the listing of each newest snapshot and the backup parent.
    assert!(
        data_reads(&idle) <= 8,
        "idle round read file contents: {idle:?}"
    );
    assert!(
        data_reads(&changed) <= 8,
        "one-change round read file contents: {changed:?}"
    );
}
