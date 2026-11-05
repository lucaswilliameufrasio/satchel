//! Measure one sync round on a large folder, one process per round.
//!
//! Each command is meant to run under `/usr/bin/time -l` (macOS) or
//! `/usr/bin/time -v` (Linux), so the peak memory of that round is visible.
//! A new process per round is also what a restarted desktop does.
//!
//! ```text
//! large_folder gen   <folder> <files> <kib-each> <big-mib>
//! large_folder sync  <repository-dir> <folder> <device-hex>
//! large_folder touch <folder>
//! ```

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;

use satchel_core::{DeviceId, Location, sync_folder};

const PASSWORD: &str = "large-folder-measurement";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("gen") => generate(
            Path::new(&args[1]),
            args[2].parse().expect("files"),
            args[3].parse().expect("kib"),
            args[4].parse().expect("big mib"),
        ),
        Some("sync") => {
            let location = Location::Directory(PathBuf::from(&args[1]));
            let device = DeviceId::parse(&args[3]).expect("device id");
            let started = Instant::now();
            let outcome =
                sync_folder(&location, PASSWORD, &device, Path::new(&args[2])).expect("sync");
            println!(
                "sync wrote={} seconds={:.2}",
                outcome.wrote,
                started.elapsed().as_secs_f64()
            );
        }
        Some("touch") => {
            let path = Path::new(&args[1]).join("d000").join("f00000.bin");
            let mut file = fs::OpenOptions::new()
                .append(true)
                .open(&path)
                .expect("open");
            file.write_all(b"changed").expect("append");
            println!("touched {}", path.display());
        }
        _ => eprintln!("usage: gen | sync | touch"),
    }
}

/// `files` files of `kib` KiB in 100 folders, and one file of `big` MiB.
/// The bytes are pseudo-random so neither compression nor dedup hides the size.
fn generate(folder: &Path, files: usize, kib: usize, big: usize) {
    let mut state = 0x9E37_79B9_7F4A_7C15_u64;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    let mut fill = |len: usize| {
        let mut bytes = Vec::with_capacity(len + 8);
        while bytes.len() < len {
            bytes.extend_from_slice(&next().to_le_bytes());
        }
        bytes.truncate(len);
        bytes
    };
    for index in 0..files {
        let dir = folder.join(format!("d{:03}", index % 100));
        fs::create_dir_all(&dir).expect("dir");
        fs::write(dir.join(format!("f{index:05}.bin")), fill(kib * 1024)).expect("file");
    }
    if big > 0 {
        let mut file = fs::File::create(folder.join("big.bin")).expect("big");
        for _ in 0..big {
            file.write_all(&fill(1024 * 1024)).expect("write");
        }
    }
    println!("generated {files} files of {kib} KiB and {big} MiB");
}
