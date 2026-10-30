use std::net::{SocketAddr, TcpStream};
use std::thread;
use std::time::{Duration, Instant};

use satchel_pair::{
    Decision, DeviceSecrets, Error, JoinRequest, ListenConfig, PairingSession,
    is_local_multicast_interface,
};

fn listen(timeout: Duration, advertise: bool) -> PairingSession {
    let bind = if advertise {
        "0.0.0.0:0".parse().expect("bind")
    } else {
        "127.0.0.1:0".parse().expect("bind")
    };
    PairingSession::listen(ListenConfig {
        bind,
        timeout,
        advertise,
    })
    .expect("listen")
}

fn secrets() -> DeviceSecrets {
    DeviceSecrets {
        endpoint: "https://example.invalid/bucket".to_string(),
        bucket_credential: "bucket-fixture".to_string(),
        device_key_password: "device-key-fixture".to_string(),
    }
}

fn join_request(address: SocketAddr, code: &str, fingerprint: &str) -> JoinRequest {
    JoinRequest {
        address,
        code: code.to_string(),
        fingerprint: fingerprint.to_string(),
        device_name: "phone".to_string(),
        os: "android".to_string(),
        claimed_address: "100.64.0.2:9".to_string(),
        claimed_interface: "tailscale0".to_string(),
        timeout: Duration::from_secs(5),
    }
}

fn flip_fingerprint(fingerprint: &str) -> String {
    let mut chars = Vec::new();
    for character in fingerprint.chars() {
        chars.push(character);
    }
    if chars[0] == 'a' {
        chars[0] = 'b';
    } else {
        chars[0] = 'a';
    }
    let mut text = String::new();
    for character in chars {
        text.push(character);
    }
    text
}

fn assert_closed(port: u16) {
    let address = SocketAddr::from(([127, 0, 0, 1], port));
    let connected = TcpStream::connect_timeout(&address, Duration::from_millis(300));
    assert!(connected.is_err());
}

#[test]
fn approve_shows_the_socket_peer_and_omits_the_recovery_password() {
    let session = listen(Duration::from_secs(5), false);
    let offer = session.offer().clone();
    let address = offer.address;
    let code = offer.code.clone();
    let fingerprint = offer.fingerprint.clone();
    let joiner = thread::spawn(move || join_satchel(address, &code, &fingerprint));
    let approval = session
        .recv_approval(Duration::from_secs(5))
        .expect("approval");
    assert_eq!(approval.device_name, "phone");
    assert_eq!(approval.os, "android");
    assert_eq!(approval.code, offer.code);
    assert!(approval.peer_address.ip().is_loopback());
    assert!(!approval.peer_address.to_string().contains("100.64.0.2"));
    assert_ne!(approval.local_interface, "tailscale0");
    assert_eq!(approval.local_interface, "lo");
    session
        .decide(Decision::Approve(secrets()))
        .expect("decide");
    let received = joiner.join().expect("join thread").expect("grant");
    assert_eq!(received, secrets());
    let dumped = format!("{received:?}");
    assert!(!dumped.contains("recovery"));
    let port = offer.address.port();
    let outcome = session.wait().expect("outcome");
    assert!(outcome.secrets_sent);
    assert_eq!(outcome.holder_frames, 1);
    assert_closed(port);
}

#[test]
fn deny_sends_no_grant() {
    let session = listen(Duration::from_secs(5), false);
    let offer = session.offer().clone();
    let joiner =
        thread::spawn(move || join_satchel(offer.address, &offer.code, &offer.fingerprint));
    let _approval = session
        .recv_approval(Duration::from_secs(5))
        .expect("approval");
    session.decide(Decision::Deny).expect("deny");
    let err = joiner.join().expect("join thread").expect_err("denied");
    assert_eq!(err, Error::Denied);
    let outcome = session.wait().expect("finished");
    assert!(!outcome.secrets_sent);
    assert_eq!(outcome.holder_frames, 0);
}

#[test]
fn wrong_code_never_reaches_approval() {
    let session = listen(Duration::from_secs(5), false);
    let offer = session.offer().clone();
    let wrong = if offer.code == "000 000" {
        "000 001".to_string()
    } else {
        "000 000".to_string()
    };
    let err = join_satchel(offer.address, &wrong, &offer.fingerprint).expect_err("rejected");
    assert_eq!(err, Error::CodeRejected);
    assert!(!session.approval_sent());
    let holder = session.wait().expect_err("holder rejected");
    assert_eq!(holder, Error::CodeRejected);
}

#[test]
fn wrong_fingerprint_never_reaches_approval() {
    let session = listen(Duration::from_secs(5), false);
    let offer = session.offer().clone();
    let wrong = flip_fingerprint(&offer.fingerprint);
    let err = join_satchel(offer.address, &offer.code, &wrong).expect_err("fingerprint");
    assert_eq!(err, Error::Fingerprint);
    assert!(!session.approval_sent());
    let _ = session.wait();
}

#[test]
fn dropping_the_session_closes_the_port() {
    let session = listen(Duration::from_secs(5), false);
    let port = session.offer().address.port();
    let address = session.offer().address;
    let probe = TcpStream::connect_timeout(&address, Duration::from_millis(300));
    assert!(probe.is_ok());
    drop(probe);
    drop(session);
    assert_closed(port);
}

#[test]
fn attempt_times_out_without_a_joiner() {
    let started = Instant::now();
    let session = listen(Duration::from_millis(400), false);
    let err = session.wait().expect_err("timeout");
    assert_eq!(err, Error::TimedOut);
    assert!(started.elapsed() < Duration::from_secs(3));
    assert!(started.elapsed() >= Duration::from_millis(300));
}

#[test]
fn typed_address_does_not_require_mdns() {
    let session = listen(Duration::from_secs(5), false);
    assert!(!session.offer().advertised);
    let offer = session.offer().clone();
    assert!(offer.invite.contains(&offer.fingerprint));
    assert!(!offer.invite.contains("recovery"));
    let joiner =
        thread::spawn(move || join_satchel(offer.address, &offer.code, &offer.fingerprint));
    let _approval = session
        .recv_approval(Duration::from_secs(5))
        .expect("approval");
    session
        .decide(Decision::Approve(secrets()))
        .expect("decide");
    joiner.join().expect("thread").expect("grant");
    session.wait().expect("done");
}

#[test]
fn announcement_lives_only_while_the_session_does() {
    let names = if_addrs::get_if_addrs().expect("interfaces");
    let mut local = false;
    for iface in &names {
        if is_local_multicast_interface(&iface.name) && !iface.is_loopback() {
            local = true;
            break;
        }
    }
    let session = listen(Duration::from_secs(3), true);
    let port = session.offer().address.port();
    if local {
        assert!(session.offer().advertised);
    } else {
        assert!(!session.offer().advertised);
    }
    drop(session);
    assert_closed(port);
}

fn join_satchel(
    address: SocketAddr,
    code: &str,
    fingerprint: &str,
) -> Result<DeviceSecrets, Error> {
    satchel_pair::join(join_request(address, code, fingerprint))
}
