//! The service's work, shared by service mode and `--console`.

use std::fs::OpenOptions;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use mixlink_core::config::DisplaySettings;

use crate::discovery::{self, DiscoverySettings};
use crate::display;
use crate::error::SvcError;
use crate::input::{open_feeder, pads};
use crate::pipe::server::listen;
use crate::pipe::session::{NoPairing, PairingPort};
use crate::platform::windows::pad_nodes::WindowsPadHost;
use crate::platform::windows::pipe::create_instance;
use crate::platform::windows::security::PipeSecurity;
use crate::session_listen::{self, SessionSettings};
use crate::stop::StopSignal;
use mixlink_input::platform::windows::vhid::VHid;

/// How long `run` keeps poking the listener awake after a stop.
const WAKE_TIMEOUT: Duration = Duration::from_secs(2);
const WAKE_RETRY: Duration = Duration::from_millis(20);

#[derive(Debug, Clone)]
pub struct RunOptions {
    pub pipe: String,
    /// `Some`: start the virtual display (spec 0015). Tests that only exercise the pipe pass `None`.
    pub display: Option<DisplaySettings>,
    /// Open the virtual input driver and hold it as its feeder (spec 0016). Pipe-only tests pass `false`.
    pub input: bool,
    /// `Some`: advertise the host on the LAN (spec 0017). Tests pass `None`.
    pub discovery: Option<DiscoverySettings>,
    /// `Some`: pairing and secure sessions on the session port (spec 0018). Tests pass `None`.
    pub session: Option<SessionSettings>,
}

/// Serves the pipe until `stop` is set. `on_ready` runs once the pipe accepts clients;
/// errors before that (pipe taken, bad ACL) are returned without calling it.
pub fn run(
    stop: &Arc<StopSignal>,
    opts: &RunOptions,
    on_ready: impl FnOnce(),
) -> Result<(), SvcError> {
    let started = Instant::now();
    let security = Arc::new(PipeSecurity::new()?);
    let first = create_instance(&opts.pipe, true, &security)?.ok_or(SvcError::PipeTaken)?;
    // Before the session listener: streams ask this thread for the client monitor (spec 0020).
    let display = match opts.display {
        Some(settings) => Some(display::service::spawn(Arc::clone(stop), settings)?),
        None => None,
    };
    let session = opts.session.clone().and_then(|mut settings| {
        settings.display = display.as_ref().map(|(_, handle)| handle.clone());
        match session_listen::start(Arc::clone(stop), settings) {
            Ok(sessions) => {
                log::info!("session listener on {}", sessions.addr);
                Some(sessions)
            }
            Err(error) => {
                log::error!("{error}");
                None
            }
        }
    });
    let pairing: Arc<dyn PairingPort + Send + Sync> = match &session {
        Some(sessions) => Arc::clone(&sessions.shared) as Arc<dyn PairingPort + Send + Sync>,
        None => Arc::new(NoPairing),
    };
    let listener = {
        let name = opts.pipe.clone();
        let stop = Arc::clone(stop);
        let security = Arc::clone(&security);
        thread::Builder::new()
            .name("svc-pipe-listen".into())
            .spawn(move || listen(&name, first, &security, &stop, started, &pairing))?
    };
    log::info!("serving {}", opts.pipe);
    // Pad nodes left by a crash go first; pads come and go per session (spec 0016).
    if opts.input
        && let Err(error) = pads::remove_all(&mut WindowsPadHost)
    {
        log::warn!("leftover virtual Xbox controllers not removed: {error}");
    }
    // Held until `run` returns; closing it releases every key and button (spec 0016).
    let _feeder = opts.input.then(open_input).flatten();
    // Dropped when `run` returns: stops the responder and sends the mDNS goodbye.
    let _advertiser = opts.discovery.clone().and_then(discovery::start);
    on_ready();
    stop.wait();
    wake_listener(&opts.pipe, &listener);
    if listener.join().is_err() {
        log::error!("pipe listener panicked");
    }
    if let Some((display, _)) = display
        && display.join().is_err()
    {
        log::error!("display thread panicked");
    }
    if let Some(sessions) = session
        && sessions.thread.join().is_err()
    {
        log::error!("session thread panicked");
    }
    log::info!("stopped");
    Ok(())
}

/// The feeder, or `None` with a warning: without the driver the service still runs (the `SendInput`
/// fallback comes with the input path).
fn open_input() -> Option<VHid> {
    match open_feeder() {
        Ok(vhid) => {
            log::info!("virtual input ready");
            Some(vhid)
        }
        Err(error) => {
            log::warn!("virtual input unavailable: {error}");
            None
        }
    }
}

/// The listener blocks in `ConnectNamedPipe`; a throwaway client connection lets it see the stop.
/// Retried because the listener may be between instances when the first attempt runs.
fn wake_listener(name: &str, listener: &thread::JoinHandle<()>) {
    let deadline = Instant::now() + WAKE_TIMEOUT;
    while !listener.is_finished() && Instant::now() < deadline {
        let _ = OpenOptions::new().read(true).write(true).open(name);
        thread::sleep(WAKE_RETRY);
    }
}

#[cfg(test)]
#[path = "run_tests.rs"]
mod tests;
