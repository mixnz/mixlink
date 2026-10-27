//! Pairing from the launcher (spec 0018): thread `client-pair` runs one pairing with a host,
//! the UI follows it through `pair-state` events and sends the PIN with `pair_submit`.

use std::net::{IpAddr, SocketAddrV4};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use mixlink_core::config::MAX_NAME_BYTES;
use mixlink_core::ids::DeviceId;
use mixlink_net::client_store;
use mixlink_net::control_link::{ControlLink, Wire};
use mixlink_net::error::NetError;
use mixlink_net::pairing::PairError;
use mixlink_net::pairing::client::{self, PairContext};
use mixlink_net::{Transport, TransportConfig};
use mixlink_proto::channel::Channel;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::hosts::HostsState;
use crate::strings;

pub const PAIR_EVENT: &str = "pair-state";

/// Why pairing stopped; the UI picks the text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FailReason {
    /// The prompt on the host closed: expired, cancelled there, or too many wrong PINs.
    Closed,
    Busy,
    NoAnswer,
    Version,
    Failed,
}

/// What the UI shows for the pairing in progress. `host` is the host id (32 hex).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum PairState {
    Idle,
    Requesting {
        host: String,
    },
    AwaitingPin {
        host: String,
        expires_s: u32,
        wrong_pin: bool,
    },
    Checking {
        host: String,
    },
    Paired {
        host: String,
    },
    Failed {
        host: String,
        reason: FailReason,
    },
}

/// Six digits, spaces around them ignored; anything else is refused before it costs an attempt.
pub fn parse_pin(text: &str) -> Option<u32> {
    let text = text.trim();
    if text.len() != 6 || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

pub fn fail_reason(error: &PairError) -> FailReason {
    match error {
        PairError::Closed | PairError::BadPin => FailReason::Closed,
        PairError::Busy => FailReason::Busy,
        PairError::Version => FailReason::Version,
        PairError::Net(NetError::Timeout) => FailReason::NoAnswer,
        PairError::Net(_) => FailReason::Failed,
    }
}

/// Paired host ids saved in `dir`.
pub fn known_ids_in(dir: &Path) -> Vec<DeviceId> {
    client_store::load_known(dir)
        .into_iter()
        .map(|known| known.host)
        .collect()
}

/// Paired host ids for this user; empty without a profile folder.
pub fn known_ids() -> Vec<DeviceId> {
    client_store::default_dir()
        .map(|dir| known_ids_in(&dir))
        .unwrap_or_default()
}

fn store_dir() -> Result<PathBuf, String> {
    client_store::default_dir().ok_or_else(|| strings::NO_PROFILE_FOLDER.to_owned())
}

/// This PC's name as hosts see it; at most 63 bytes, cut on a character boundary.
fn device_name() -> String {
    let name = std::env::var("COMPUTERNAME").unwrap_or_else(|_| strings::DEVICE_NAME.to_owned());
    let mut end = name.len().min(MAX_NAME_BYTES);
    while !name.is_char_boundary(end) {
        end -= 1;
    }
    name[..end].to_owned()
}

fn parse_id(id: &str) -> Result<DeviceId, String> {
    id.parse().map_err(|_| format!("bad host id: {id}"))
}

/// Removes a paired host from this PC (the host keeps its entry until Remove in its tray).
pub fn forget(app: &AppHandle, id: &str) -> Result<(), String> {
    let dir = store_dir()?;
    client_store::forget(&dir, parse_id(id)?).map_err(|error| error.to_string())?;
    app.state::<HostsState>().set_known(app, known_ids_in(&dir));
    Ok(())
}

#[derive(Debug)]
struct Job {
    pins: Sender<u32>,
    cancel: Arc<AtomicBool>,
}

#[derive(Debug, Default)]
pub struct PairingState {
    job: Mutex<Option<Job>>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl PairingState {
    /// Starts pairing with a discovered host; a pairing already running is cancelled.
    pub fn request(&self, app: &AppHandle, id: &str) -> Result<(), String> {
        let host = parse_id(id)?;
        let entry = app
            .state::<HostsState>()
            .find(&host)
            .ok_or_else(|| strings::HOST_GONE.to_owned())?;
        let IpAddr::V4(ip) = entry.address else {
            return Err(strings::HOST_GONE.to_owned());
        };
        self.cancel(app);
        let (pins, inbox) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let job = PairJob {
            app: app.clone(),
            addr: SocketAddrV4::new(ip, entry.port),
            host,
            host_name: entry.name,
            cancel: Arc::clone(&cancel),
        };
        thread::Builder::new()
            .name("client-pair".into())
            .spawn(move || job.run(&inbox))
            .map_err(|error| error.to_string())?;
        *lock(&self.job) = Some(Job { pins, cancel });
        Ok(())
    }

    pub fn submit(&self, pin: &str) -> Result<(), String> {
        let pin = parse_pin(pin).ok_or_else(|| strings::PIN_FORMAT.to_owned())?;
        let job = lock(&self.job);
        let job = job
            .as_ref()
            .ok_or_else(|| strings::NOT_PAIRING.to_owned())?;
        job.pins
            .send(pin)
            .map_err(|_| strings::NOT_PAIRING.to_owned())
    }

    pub fn cancel(&self, app: &AppHandle) {
        if let Some(job) = lock(&self.job).take() {
            job.cancel.store(true, Ordering::Release);
            emit(app, &PairState::Idle);
        }
    }
}

fn emit(app: &AppHandle, state: &PairState) {
    if let Err(error) = app.emit(PAIR_EVENT, state) {
        log::warn!("emitting pair state: {error}");
    }
}

struct PairJob {
    app: AppHandle,
    addr: SocketAddrV4,
    host: DeviceId,
    host_name: String,
    cancel: Arc<AtomicBool>,
}

impl PairJob {
    fn emit(&self, state: &PairState) {
        if !self.cancel.load(Ordering::Acquire) {
            emit(&self.app, state);
        }
    }

    fn fail(&self, reason: FailReason) {
        self.emit(&PairState::Failed {
            host: self.host.to_string(),
            reason,
        });
    }

    fn run(self, pins: &Receiver<u32>) {
        let host = self.host.to_string();
        self.emit(&PairState::Requesting { host: host.clone() });
        if let Err(error) = self.pair(pins, &host)
            && !matches!(error, PairError::Net(NetError::Cancelled))
        {
            log::info!("pairing with {host} stopped: {error}");
            self.fail(fail_reason(&error));
        }
    }

    fn pair(&self, pins: &Receiver<u32>, host: &str) -> Result<(), PairError> {
        let dir = store_dir().map_err(NetError::Store)?;
        let me = client_store::load_or_create_identity(&dir)?;
        let (_transport, mut lanes) = Transport::start(TransportConfig::client())?;
        let missing = || NetError::Store("control lane missing".to_owned());
        let mut tx = lanes.take_tx(Channel::Control).ok_or_else(missing)?;
        let mut rx = lanes.take_rx(Channel::Control).ok_or_else(missing)?;
        let mut wire = Wire {
            tx: &mut tx,
            rx: &mut rx,
            cancel: &self.cancel,
        };
        let ctx = PairContext {
            me,
            name: device_name(),
            host: self.host,
            host_name: self.host_name.clone(),
        };
        let mut link = ControlLink::new(self.addr);
        let expires_s = client::request(&mut link, &mut wire, &ctx)?;
        let deadline = Instant::now() + Duration::from_secs(u64::from(expires_s));
        let mut wrong_pin = false;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            self.emit(&PairState::AwaitingPin {
                host: host.to_owned(),
                expires_s: u32::try_from(left.as_secs()).unwrap_or(expires_s),
                wrong_pin,
            });
            let pin = wait_pin(pins, deadline, &mut link, &mut wire)?;
            self.emit(&PairState::Checking {
                host: host.to_owned(),
            });
            match client::submit(&mut link, &mut wire, &ctx, pin) {
                Ok(known) => {
                    client_store::remember(&dir, known)?;
                    self.app
                        .state::<HostsState>()
                        .set_known(&self.app, known_ids_in(&dir));
                    self.emit(&PairState::Paired {
                        host: host.to_owned(),
                    });
                    return Ok(());
                }
                Err(PairError::BadPin) => wrong_pin = true,
                Err(error) => return Err(error),
            }
        }
    }
}

/// How often the host is checked for a cancelled prompt while the user types the PIN.
const CLOSE_POLL: Duration = Duration::from_millis(100);

/// The PIN the user types before `deadline`; `Closed` once the host cancels or time runs out.
fn wait_pin(
    pins: &Receiver<u32>,
    deadline: Instant,
    link: &mut ControlLink,
    wire: &mut Wire<'_>,
) -> Result<u32, PairError> {
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        match pins.recv_timeout(left.min(CLOSE_POLL)) {
            Ok(pin) => return Ok(pin),
            Err(RecvTimeoutError::Disconnected) => return Err(PairError::Net(NetError::Cancelled)),
            Err(RecvTimeoutError::Timeout) if left <= CLOSE_POLL => return Err(PairError::Closed),
            Err(RecvTimeoutError::Timeout) => {
                if client::closed_by_host(link, wire) {
                    return Err(PairError::Closed);
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "pairing_tests.rs"]
mod tests;
