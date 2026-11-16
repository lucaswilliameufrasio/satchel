//! Wake notices on the phone, only while the app is open.
//!
//! The app calls [`start`] when it comes to the foreground and [`stop`] when it
//! leaves. While started, a thread waits on the mesh address and [`wait`]
//! returns when a peer said it wrote a snapshot. A round through a grant
//! announces the endpoint when it is new, and sends notices when it wrote.
//! Outside the foreground nothing listens: a peer's notice is dropped and the
//! next open catches up.

use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use satchel_core::{DeviceId, SyncOutcome, WakeEndpoint, bind_saved_wake, send_wakes};

/// Wakes closer together than this are merged into one.
const GAP: Duration = Duration::from_secs(5);

#[derive(Default)]
struct Hits {
    count: u64,
    taken: u64,
    last_return: Option<Instant>,
}

struct Ear {
    endpoint: WakeEndpoint,
    /// Holds `address|key` once a snapshot carried this endpoint.
    marker: PathBuf,
    stop: Arc<AtomicBool>,
    hits: Arc<(Mutex<Hits>, Condvar)>,
    thread: std::thread::JoinHandle<()>,
}

static EAR: Mutex<Option<Ear>> = Mutex::new(None);
/// The thread of a stopped listener, which still holds the port for up to half a second.
static STOPPING: Mutex<Vec<std::thread::JoinHandle<()>>> = Mutex::new(Vec::new());

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poison) => poison.into_inner(),
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0)
}

fn marker_text(endpoint: &WakeEndpoint) -> String {
    format!("{}|{}", endpoint.address, endpoint.key)
}

/// Listen in `state_dir`, a private folder the app owns and does not back up.
///
/// `ip` empty means the mesh address; none up means no listening. Returns the
/// address listened on, or an empty string. A second call replaces the first.
pub(crate) fn start(state_dir: &Path, ip: &str) -> String {
    stop();
    // Wait for the old socket to close so the saved port is free again.
    let stopping: Vec<_> = lock(&STOPPING).drain(..).collect();
    for thread in stopping {
        let _ = thread.join();
    }
    let ip: Option<IpAddr> = if ip.is_empty() {
        satchel_pair::mesh_ip()
    } else {
        ip.parse().ok()
    };
    let Some(ip) = ip else {
        return String::new();
    };
    let Ok((mut listener, endpoint)) = bind_saved_wake(&state_dir.join("wake.json"), ip) else {
        return String::new();
    };
    let stop_flag = Arc::new(AtomicBool::new(false));
    let hits = Arc::new((Mutex::new(Hits::default()), Condvar::new()));
    let thread_stop = Arc::clone(&stop_flag);
    let thread_hits = Arc::clone(&hits);
    let thread = std::thread::spawn(move || {
        while !thread_stop.load(Ordering::SeqCst) {
            let started = Instant::now();
            if listener.wait(Duration::from_millis(500), now).is_some() {
                let (state, signal) = &*thread_hits;
                lock(state).count += 1;
                signal.notify_all();
            } else if started.elapsed() < Duration::from_millis(100) {
                std::thread::sleep(Duration::from_millis(500));
            }
        }
    });
    let address = endpoint.address.clone();
    *lock(&EAR) = Some(Ear {
        endpoint,
        marker: state_dir.join("wake.announced"),
        stop: stop_flag,
        hits,
        thread,
    });
    address
}

/// Stop listening. Returns at once; the socket closes within half a second.
pub(crate) fn stop() {
    if let Some(ear) = lock(&EAR).take() {
        ear.stop.store(true, Ordering::SeqCst);
        ear.hits.1.notify_all();
        lock(&STOPPING).push(ear.thread);
    }
}

/// Wait up to `timeout` for a notice. False on timeout or when not listening.
///
/// Notices within [`GAP`] of the last true return are held until the gap ends.
pub(crate) fn wait(timeout: Duration) -> bool {
    let Some((hits, stop_flag)) = lock(&EAR)
        .as_ref()
        .map(|ear| (Arc::clone(&ear.hits), Arc::clone(&ear.stop)))
    else {
        return false;
    };
    let deadline = Instant::now() + timeout;
    let (state, signal) = &*hits;
    let mut guard = lock(state);
    loop {
        if stop_flag.load(Ordering::SeqCst) {
            return false;
        }
        let ready_at = guard.last_return.map(|last| last + GAP);
        let now = Instant::now();
        if guard.count > guard.taken && ready_at.is_none_or(|at| now >= at) {
            guard.taken = guard.count;
            guard.last_return = Some(now);
            return true;
        }
        if now >= deadline {
            return false;
        }
        let mut until = deadline;
        if guard.count > guard.taken
            && let Some(at) = ready_at
        {
            until = until.min(at);
        }
        guard = match signal.wait_timeout(guard, until - now) {
            Ok((guard, _)) => guard,
            Err(poison) => poison.into_inner().0,
        };
    }
}

/// The endpoint a round must announce: set only when no snapshot carried it yet.
pub(crate) fn to_announce() -> Option<WakeEndpoint> {
    let ear = lock(&EAR);
    let ear = ear.as_ref()?;
    let published = std::fs::read_to_string(&ear.marker).unwrap_or_default();
    if published == marker_text(&ear.endpoint) {
        return None;
    }
    Some(ear.endpoint.clone())
}

/// After a round: remember an announced endpoint and tell peers when it wrote.
pub(crate) fn after_sync(
    device: &DeviceId,
    announced: Option<&WakeEndpoint>,
    outcome: &SyncOutcome,
) -> usize {
    if let Some(endpoint) = announced
        && outcome.wrote
        && let Some(ear) = lock(&EAR).as_ref()
        && &ear.endpoint == endpoint
    {
        let _ = std::fs::write(&ear.marker, marker_text(endpoint));
    }
    if !outcome.wrote {
        return 0;
    }
    send_wakes(device, &outcome.peers, now())
}
