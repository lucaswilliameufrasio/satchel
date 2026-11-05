//! Send or wait for one wake notice, to check a mesh path by hand.
//!
//! `wake_probe key` prints a fresh test key.
//! `wake_probe listen <ip:port> <key> <seconds>` waits and prints the sender.
//! `wake_probe send <ip:port> <key>` sends one notice from a fixed test id.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use satchel_core::{DeviceId, WakeEndpoint, WakeListener, WakePeer, generate_wake_key, send_wakes};

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("key") => println!("{}", generate_wake_key()),
        Some("listen") if args.len() == 4 => {
            let address = args[1].parse().expect("ip:port");
            let seconds = args[3].parse().expect("seconds");
            let mut listener = WakeListener::bind(address, &args[2]).expect("bind");
            println!("listening {}", listener.local_addr().expect("addr"));
            match listener.wait(Duration::from_secs(seconds), now) {
                Some(sender) => println!("woken by {sender}"),
                None => println!("nothing"),
            }
        }
        Some("send") if args.len() == 3 => {
            let sender = DeviceId::parse("0123456789abcdef0123456789abcdef").expect("id");
            let peer = WakePeer {
                device_id: "fedcba9876543210fedcba9876543210".to_string(),
                endpoint: WakeEndpoint {
                    address: args[1].clone(),
                    key: args[2].clone(),
                },
            };
            println!("sent {}", send_wakes(&sender, &[peer], now()));
        }
        _ => eprintln!("usage: wake_probe key | listen <ip:port> <key> <s> | send <ip:port> <key>"),
    }
}
