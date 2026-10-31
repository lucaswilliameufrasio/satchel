use std::io::{ErrorKind, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use spake2::{Ed25519Group, Identity, Password, Spake2};
use subtle::ConstantTimeEq;

use crate::Error;
use crate::announce::{Invite, announcement_record, interface_for, random_alias};
use crate::code::{code_digits, format_code, generate_code, parse_code};
use crate::tls::{self, SessionCert, client_stream, map_client, map_io, server_stream};

const MAX_FRAME: usize = 64 * 1024;
const ID_HOLDER: &[u8] = b"satchel-holder";
const ID_JOINER: &[u8] = b"satchel-joiner";
const CONFIRM_HOLDER: &[u8] = b"satchel-holder-confirm";
const CONFIRM_JOINER: &[u8] = b"satchel-joiner-confirm";

/// How long the add-device screen listens when the caller does not pick a shorter test deadline.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(180);

/// What the holder shows, and what a typed or QR invite carries.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Offer {
    /// Six digits with a space, for example `482 193`.
    pub code: String,
    /// Random session alias. It is not a hostname.
    pub alias: String,
    /// SHA-256 of the certificate DER, lowercase hex.
    pub fingerprint: String,
    /// Address to type when multicast does not reach the other device.
    pub address: SocketAddr,
    /// True only while an mDNS announcement was registered.
    pub advertised: bool,
    /// JSON invite. It has no recovery password.
    pub invite: String,
}

/// Where to listen, for how long, and whether to announce on the local network.
pub struct ListenConfig {
    /// Use `127.0.0.1` for a typed-address test. `0.0.0.0` also accepts the local network.
    pub bind: SocketAddr,
    /// The attempt ends at this deadline even if nobody connects.
    pub timeout: Duration,
    /// Announce only while this session is alive. Mesh pairing leaves this false.
    pub advertise: bool,
}

/// What the holder shows before any secret moves. The address comes from the socket.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApprovalRequest {
    /// Name the joiner chose.
    pub device_name: String,
    /// Operating system the joiner reported.
    pub os: String,
    /// Opaque id the joiner will use in its key file.
    pub device_id: String,
    /// `peer_addr()` of the accepted socket.
    pub peer_address: SocketAddr,
    /// Local interface that owns the socket's local address.
    pub local_interface: String,
    /// The same spaced code the joiner typed.
    pub code: String,
}

/// Sent only after approval. The recovery password is not here.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceSecrets {
    /// Repository endpoint or path.
    pub endpoint: String,
    /// Credential that can use the bucket. Not the recovery password.
    pub bucket_credential: String,
    /// Password for this device's own key.
    pub device_key_password: String,
}

/// Holder's answer. `Deny` writes no frame.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Decision {
    /// Send the three fields and nothing else.
    Approve(DeviceSecrets),
    /// Close without a grant.
    Deny,
}

/// What the joiner knows before the grant.
pub struct JoinRequest {
    /// Typed mesh address, or the address from the announcement.
    pub address: SocketAddr,
    /// Six digits, with or without the display space.
    pub code: String,
    /// Fingerprint the screen or the TXT record showed.
    pub fingerprint: String,
    /// Shown on the approval screen.
    pub device_name: String,
    /// Shown on the approval screen.
    pub os: String,
    /// Opaque device id. It is not a secret and it is not a hostname.
    pub device_id: String,
    /// Ignored by the holder. Present so a test can prove the socket address wins.
    pub claimed_address: String,
    /// Ignored by the holder.
    pub claimed_interface: String,
    /// Socket deadline for this attempt.
    pub timeout: Duration,
}

/// Result of one holder attempt that finished without a protocol error.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PairingOutcome {
    /// True only after the grant frame was written.
    pub secrets_sent: bool,
    /// Grant frames written. A denial stays at zero.
    pub holder_frames: u8,
}

/// One pairing attempt. Dropping it stops listening and stops the announcement.
pub struct PairingSession {
    offer: Offer,
    approval_rx: Mutex<Receiver<ApprovalRequest>>,
    decision_tx: Mutex<Option<Sender<Decision>>>,
    shutdown: Arc<AtomicBool>,
    outcome: Arc<Mutex<Option<Result<PairingOutcome, Error>>>>,
    approval_sent: Arc<AtomicBool>,
    thread: Mutex<Option<JoinHandle<()>>>,
    advertisement: Mutex<Option<Advertisement>>,
}

impl PairingSession {
    /// Bind, and announce only when `config.advertise` is set and a local interface exists.
    pub fn listen(config: ListenConfig) -> Result<Self, Error> {
        if config.timeout.is_zero() {
            return Err(Error::TimedOut);
        }
        let listener = TcpListener::bind(config.bind).map_err(map_io)?;
        listener.set_nonblocking(true).map_err(map_io)?;
        let address = listener.local_addr().map_err(map_io)?;
        let code_value = generate_code();
        let code = format_code(code_value)?;
        let digits = code_digits(code_value)?;
        let cert = generate_session_cert()?;
        let alias = random_alias();
        let invite = Invite::new(&digits, address.to_string(), &cert.fingerprint)?.encode()?;
        let advertisement = if config.advertise {
            start_advertisement(&alias, address.port(), &cert.fingerprint)
        } else {
            None
        };
        let offer = Offer {
            code: code.clone(),
            alias,
            fingerprint: cert.fingerprint.clone(),
            address,
            advertised: advertisement.is_some(),
            invite,
        };
        let (approval_tx, approval_rx) = channel();
        let (decision_tx, decision_rx) = channel();
        let shutdown = Arc::new(AtomicBool::new(false));
        let outcome = Arc::new(Mutex::new(None));
        let approval_sent = Arc::new(AtomicBool::new(false));
        let thread_shutdown = Arc::clone(&shutdown);
        let thread_outcome = Arc::clone(&outcome);
        let thread_approval = Arc::clone(&approval_sent);
        let deadline = Instant::now() + config.timeout;
        let thread = thread::Builder::new()
            .name("satchel-pair".to_string())
            .spawn(move || {
                run_holder(HolderJob {
                    listener,
                    cert,
                    code_value,
                    code_text: code,
                    deadline,
                    shutdown: thread_shutdown,
                    approval_sent: thread_approval,
                    outcome: thread_outcome,
                    approval_tx,
                    decision_rx,
                });
            })
            .map_err(|_| Error::Network)?;
        Ok(Self {
            offer,
            approval_rx: Mutex::new(approval_rx),
            decision_tx: Mutex::new(Some(decision_tx)),
            shutdown,
            outcome,
            approval_sent,
            thread: Mutex::new(Some(thread)),
            advertisement: Mutex::new(advertisement),
        })
    }

    /// The code, fingerprint, and typed address.
    pub fn offer(&self) -> &Offer {
        &self.offer
    }

    /// True after PAKE and the hello, at the moment the approval screen is fed.
    pub fn approval_sent(&self) -> bool {
        self.approval_sent.load(Ordering::SeqCst)
    }

    /// Wait until the joiner has passed the code check.
    pub fn recv_approval(&self, timeout: Duration) -> Result<ApprovalRequest, Error> {
        let deadline = Instant::now() + timeout;
        loop {
            {
                let receiver = lock(&self.approval_rx);
                match receiver.try_recv() {
                    Ok(request) => return Ok(request),
                    Err(TryRecvError::Empty) => {}
                    Err(TryRecvError::Disconnected) => return Err(self.current_error()),
                }
            }
            if Instant::now() >= deadline {
                return Err(Error::TimedOut);
            }
            thread::sleep(Duration::from_millis(20));
        }
    }

    /// Non-blocking poll for the approval screen.
    pub fn try_approval(&self) -> Result<Option<ApprovalRequest>, Error> {
        let receiver = lock(&self.approval_rx);
        match receiver.try_recv() {
            Ok(request) => Ok(Some(request)),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => Err(self.current_error()),
        }
    }

    /// Approve or deny. A denial does not write a grant.
    pub fn decide(&self, decision: Decision) -> Result<(), Error> {
        let sender = lock(&self.decision_tx).take();
        let Some(sender) = sender else {
            return Err(Error::Closed);
        };
        sender.send(decision).map_err(|_| Error::Closed)
    }

    /// Join the attempt. An undecided approval is treated as a denial.
    pub fn wait(self) -> Result<PairingOutcome, Error> {
        {
            let _sender = lock(&self.decision_tx).take();
        }
        let handle = lock(&self.thread).take();
        if let Some(handle) = handle {
            let _ = handle.join();
        }
        match lock(&self.outcome).take() {
            Some(result) => result,
            None => Err(Error::Closed),
        }
    }

    fn current_error(&self) -> Error {
        match *lock(&self.outcome) {
            Some(Err(err)) => err,
            Some(Ok(_)) => Error::Closed,
            None => Error::Closed,
        }
    }
}

impl Drop for PairingSession {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::SeqCst);
        {
            let _advertisement = lock(&self.advertisement).take();
        }
        {
            let _sender = lock(&self.decision_tx).take();
        }
        let handle = lock(&self.thread).take();
        if let Some(handle) = handle {
            let _ = handle.join();
        }
    }
}

/// Dial a holder that is already listening. The fingerprint must match the certificate.
pub fn join(request: JoinRequest) -> Result<DeviceSecrets, Error> {
    if !valid_label(&request.device_name) || !valid_label(&request.os) {
        return Err(Error::Message);
    }
    if !valid_device_id(&request.device_id) {
        return Err(Error::Message);
    }
    if !valid_claim(&request.claimed_address) || !valid_claim(&request.claimed_interface) {
        return Err(Error::Message);
    }
    let code_value = parse_code(&request.code)?;
    let digits = code_digits(code_value)?;
    let sock = TcpStream::connect_timeout(&request.address, request.timeout).map_err(map_io)?;
    prepare_socket(&sock, request.timeout)?;
    let mut handshake = client_stream(sock, &request.fingerprint)?;
    let flag = Arc::clone(&handshake.mismatched);
    joiner_pake(&mut handshake.stream, &digits, &flag)?;
    let hello = Hello {
        v: 1,
        device_name: request.device_name,
        os: request.os,
        device_id: request.device_id,
        claimed_address: request.claimed_address,
        claimed_interface: request.claimed_interface,
    };
    let frame = serde_json::to_vec(&hello).map_err(|_| Error::Message)?;
    write_frame(&mut handshake.stream, &frame, |err| map_client(&flag, err))?;
    let grant = match read_frame(&mut handshake.stream, |err| map_client(&flag, err)) {
        Ok(frame) => frame,
        Err(Error::Closed) => return Err(Error::Denied),
        Err(err) => return Err(err),
    };
    decode_grant(&grant)
}

fn generate_session_cert() -> Result<SessionCert, Error> {
    tls::generate_cert()
}

struct HolderJob {
    listener: TcpListener,
    cert: SessionCert,
    code_value: u32,
    code_text: String,
    deadline: Instant,
    shutdown: Arc<AtomicBool>,
    approval_sent: Arc<AtomicBool>,
    outcome: Arc<Mutex<Option<Result<PairingOutcome, Error>>>>,
    approval_tx: Sender<ApprovalRequest>,
    decision_rx: Receiver<Decision>,
}

fn run_holder(job: HolderJob) {
    let result = attempt(&job);
    *lock(&job.outcome) = Some(result);
}

fn attempt(job: &HolderJob) -> Result<PairingOutcome, Error> {
    let accepted = accept_one(&job.listener, job.deadline, &job.shutdown)?;
    let (sock, peer) = accepted;
    let remaining = job.deadline.saturating_duration_since(Instant::now());
    let budget = if remaining.is_zero() {
        Duration::from_millis(1)
    } else {
        remaining
    };
    prepare_socket(&sock, budget)?;
    let local = sock.local_addr().map_err(map_io)?;
    let mut stream = server_stream(sock, &job.cert)?;
    let digits = code_digits(job.code_value)?;
    holder_pake(&mut stream, &digits)?;
    let hello_frame = read_frame(&mut stream, map_io)?;
    let hello: Hello = serde_json::from_slice(&hello_frame).map_err(|_| Error::Message)?;
    validate_hello(&hello)?;
    let request = ApprovalRequest {
        device_name: hello.device_name,
        os: hello.os,
        device_id: hello.device_id,
        peer_address: peer,
        local_interface: interface_for(local.ip()),
        code: job.code_text.clone(),
    };
    job.approval_sent.store(true, Ordering::SeqCst);
    job.approval_tx.send(request).map_err(|_| Error::Closed)?;
    let decision = match job.decision_rx.recv() {
        Ok(decision) => decision,
        Err(_) => {
            return Ok(PairingOutcome {
                secrets_sent: false,
                holder_frames: 0,
            });
        }
    };
    match decision {
        Decision::Deny => Ok(PairingOutcome {
            secrets_sent: false,
            holder_frames: 0,
        }),
        Decision::Approve(secrets) => {
            let frame = encode_grant(&secrets)?;
            write_frame(&mut stream, &frame, map_io)?;
            Ok(PairingOutcome {
                secrets_sent: true,
                holder_frames: 1,
            })
        }
    }
}

fn accept_one(
    listener: &TcpListener,
    deadline: Instant,
    shutdown: &AtomicBool,
) -> Result<(TcpStream, SocketAddr), Error> {
    loop {
        if shutdown.load(Ordering::SeqCst) {
            return Err(Error::Closed);
        }
        if Instant::now() >= deadline {
            return Err(Error::TimedOut);
        }
        match listener.accept() {
            Ok(pair) => return Ok(pair),
            Err(err)
                if err.kind() == ErrorKind::WouldBlock || err.kind() == ErrorKind::Interrupted =>
            {
                thread::sleep(Duration::from_millis(20));
            }
            Err(err) => return Err(map_io(err)),
        }
    }
}

fn prepare_socket(sock: &TcpStream, timeout: Duration) -> Result<(), Error> {
    sock.set_nonblocking(false).map_err(map_io)?;
    sock.set_nodelay(true).map_err(map_io)?;
    sock.set_read_timeout(Some(timeout)).map_err(map_io)?;
    sock.set_write_timeout(Some(timeout)).map_err(map_io)?;
    Ok(())
}

fn holder_pake(stream: &mut impl ReadWrite, digits: &str) -> Result<(), Error> {
    let peer_message = read_frame(stream, map_io)?;
    let (message, key) = start_side(true, digits, &peer_message)?;
    write_frame(stream, &message, map_io)?;
    let tag = confirm_tag(&key, CONFIRM_HOLDER)?;
    write_frame(stream, &tag, map_io)?;
    let peer_tag = read_frame(stream, map_io)?;
    let expect = confirm_tag(&key, CONFIRM_JOINER)?;
    if !tags_match(&peer_tag, &expect) {
        return Err(Error::CodeRejected);
    }
    Ok(())
}

fn joiner_pake(stream: &mut impl ReadWrite, digits: &str, flag: &AtomicBool) -> Result<(), Error> {
    let (message, state_key) = {
        let password = Password::new(digits.as_bytes());
        let holder = Identity::new(ID_HOLDER);
        let joiner = Identity::new(ID_JOINER);
        let (state, message) = Spake2::<Ed25519Group>::start_b(&password, &holder, &joiner);
        (message, state)
    };
    write_frame(stream, &message, |err| map_client(flag, err))?;
    let peer_message = read_frame(stream, |err| map_client(flag, err))?;
    let key = state_key
        .finish(&peer_message)
        .map_err(|_| Error::CodeRejected)?;
    let holder_tag = read_frame(stream, |err| map_client(flag, err))?;
    let own_tag = confirm_tag(&key, CONFIRM_JOINER)?;
    write_frame(stream, &own_tag, |err| map_client(flag, err))?;
    let expect = confirm_tag(&key, CONFIRM_HOLDER)?;
    if !tags_match(&holder_tag, &expect) {
        return Err(Error::CodeRejected);
    }
    Ok(())
}

fn start_side(
    holder: bool,
    digits: &str,
    peer_message: &[u8],
) -> Result<(Vec<u8>, Vec<u8>), Error> {
    let password = Password::new(digits.as_bytes());
    let id_holder = Identity::new(ID_HOLDER);
    let id_joiner = Identity::new(ID_JOINER);
    let (state, message) = if holder {
        Spake2::<Ed25519Group>::start_a(&password, &id_holder, &id_joiner)
    } else {
        Spake2::<Ed25519Group>::start_b(&password, &id_holder, &id_joiner)
    };
    let key = state
        .finish(peer_message)
        .map_err(|_| Error::CodeRejected)?;
    Ok((message, key))
}

fn confirm_tag(key: &[u8], label: &[u8]) -> Result<[u8; 32], Error> {
    let mut mac = Hmac::<Sha256>::new_from_slice(key).map_err(|_| Error::CodeRejected)?;
    mac.update(label);
    let bytes = mac.finalize().into_bytes();
    let mut out = [0_u8; 32];
    out.copy_from_slice(&bytes);
    Ok(out)
}

fn tags_match(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.ct_eq(right).unwrap_u8() == 1
}

fn write_frame(
    stream: &mut impl Write,
    payload: &[u8],
    map: impl Fn(std::io::Error) -> Error,
) -> Result<(), Error> {
    if payload.is_empty() || payload.len() > MAX_FRAME {
        return Err(Error::Message);
    }
    let len = u32::try_from(payload.len()).map_err(|_| Error::Message)?;
    stream.write_all(&len.to_be_bytes()).map_err(&map)?;
    stream.write_all(payload).map_err(&map)?;
    stream.flush().map_err(&map)?;
    Ok(())
}

fn read_frame(
    stream: &mut impl Read,
    map: impl Fn(std::io::Error) -> Error,
) -> Result<Vec<u8>, Error> {
    let mut header = [0_u8; 4];
    stream.read_exact(&mut header).map_err(&map)?;
    let len = u32::from_be_bytes(header) as usize;
    if len == 0 || len > MAX_FRAME {
        return Err(Error::Message);
    }
    let mut payload = vec![0_u8; len];
    stream.read_exact(&mut payload).map_err(&map)?;
    Ok(payload)
}

trait ReadWrite: Read + Write {}

impl<T: Read + Write> ReadWrite for T {}

fn valid_label(value: &str) -> bool {
    let count = value.chars().count();
    if count == 0 || count > 64 {
        return false;
    }
    for character in value.chars() {
        if character.is_control() {
            return false;
        }
    }
    true
}

fn valid_device_id(value: &str) -> bool {
    if value.len() != 32 {
        return false;
    }
    for character in value.chars() {
        if !character.is_ascii_hexdigit() || character.is_ascii_uppercase() {
            return false;
        }
    }
    true
}

fn valid_claim(value: &str) -> bool {
    if value.chars().count() > 256 {
        return false;
    }
    for character in value.chars() {
        if character.is_control() {
            return false;
        }
    }
    true
}

fn validate_hello(hello: &Hello) -> Result<(), Error> {
    if hello.v != 1 {
        return Err(Error::Version);
    }
    if !valid_label(&hello.device_name) || !valid_label(&hello.os) {
        return Err(Error::Message);
    }
    if !valid_device_id(&hello.device_id) {
        return Err(Error::Message);
    }
    if !valid_claim(&hello.claimed_address) || !valid_claim(&hello.claimed_interface) {
        return Err(Error::Message);
    }
    Ok(())
}

fn validate_grant(secrets: &DeviceSecrets) -> Result<(), Error> {
    if !bounded_text(&secrets.endpoint, 2048) {
        return Err(Error::Message);
    }
    if !bounded_text(&secrets.bucket_credential, 4096) {
        return Err(Error::Message);
    }
    if !bounded_text(&secrets.device_key_password, 1024) {
        return Err(Error::Message);
    }
    Ok(())
}

fn bounded_text(value: &str, max: usize) -> bool {
    if value.is_empty() || value.len() > max {
        return false;
    }
    for character in value.chars() {
        if character.is_control() {
            return false;
        }
    }
    true
}

pub(crate) fn encode_grant(secrets: &DeviceSecrets) -> Result<Vec<u8>, Error> {
    validate_grant(secrets)?;
    let body = GrantBody {
        endpoint: secrets.endpoint.clone(),
        bucket_credential: secrets.bucket_credential.clone(),
        device_key_password: secrets.device_key_password.clone(),
    };
    serde_json::to_vec(&body).map_err(|_| Error::Message)
}

fn decode_grant(frame: &[u8]) -> Result<DeviceSecrets, Error> {
    let body: GrantBody = serde_json::from_slice(frame).map_err(|_| Error::Message)?;
    let secrets = DeviceSecrets {
        endpoint: body.endpoint,
        bucket_credential: body.bucket_credential,
        device_key_password: body.device_key_password,
    };
    validate_grant(&secrets)?;
    Ok(secrets)
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Hello {
    v: u8,
    device_name: String,
    os: String,
    device_id: String,
    claimed_address: String,
    claimed_interface: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct GrantBody {
    endpoint: String,
    bucket_credential: String,
    device_key_password: String,
}

struct Advertisement {
    daemon: mdns_sd::ServiceDaemon,
    fullname: String,
}

impl Drop for Advertisement {
    fn drop(&mut self) {
        if let Ok(receiver) = self.daemon.unregister(&self.fullname) {
            let _ = receiver.recv_timeout(Duration::from_millis(500));
        }
        if let Ok(receiver) = self.daemon.shutdown() {
            let _ = receiver.recv_timeout(Duration::from_millis(500));
        }
    }
}

fn start_advertisement(alias: &str, port: u16, fingerprint: &str) -> Option<Advertisement> {
    let Ok(list) = if_addrs::get_if_addrs() else {
        return None;
    };
    let mut addresses = Vec::new();
    let mut skip = Vec::new();
    for iface in list {
        if is_announce_interface(&iface) {
            addresses.push(iface.ip());
        } else {
            skip.push(iface.name);
        }
    }
    if addresses.is_empty() {
        return None;
    }
    let record = announcement_record(alias, port, fingerprint, &addresses).ok()?;
    let fullname = record.get_fullname().to_string();
    let daemon = mdns_sd::ServiceDaemon::new().ok()?;
    for name in skip {
        let _ = daemon.disable_interface(name.as_str());
    }
    if daemon.register(record).is_err() {
        let _ = daemon.shutdown();
        return None;
    }
    Some(Advertisement { daemon, fullname })
}

fn is_announce_interface(iface: &if_addrs::Interface) -> bool {
    if iface.is_loopback() {
        return false;
    }
    crate::announce::is_local_multicast_interface(&iface.name)
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poison) => poison.into_inner(),
    }
}

#[cfg(test)]
mod tests {
    use super::{DeviceSecrets, decode_grant, encode_grant};

    #[test]
    fn grant_json_has_only_the_three_approved_fields() {
        let secrets = DeviceSecrets {
            endpoint: "https://example.invalid/bucket".to_string(),
            bucket_credential: "bucket-fixture".to_string(),
            device_key_password: "device-key-fixture".to_string(),
        };
        let bytes = encode_grant(&secrets).expect("grant");
        let text = String::from_utf8(bytes.clone()).expect("utf8");
        assert!(!text.contains("recovery"));
        let value: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        let object = value.as_object().expect("object");
        let mut keys = Vec::new();
        for key in object.keys() {
            keys.push(key.clone());
        }
        keys.sort();
        assert_eq!(
            keys,
            vec![
                "bucket_credential".to_string(),
                "device_key_password".to_string(),
                "endpoint".to_string()
            ]
        );
        let mut leaked = value;
        leaked.as_object_mut().expect("object").insert(
            "recovery".to_string(),
            serde_json::Value::String("no".to_string()),
        );
        let leaked_bytes = serde_json::to_vec(&leaked).expect("bytes");
        assert!(decode_grant(&leaked_bytes).is_err());
        let empty = DeviceSecrets {
            endpoint: String::new(),
            bucket_credential: "bucket-fixture".to_string(),
            device_key_password: "device-key-fixture".to_string(),
        };
        assert!(encode_grant(&empty).is_err());
    }
}
