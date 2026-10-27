//! Thread `svc-session` (spec 0018): pairing and secure session handshakes on the session port.
//! Control plane. The pipe reads the prompt and the paired list from `PairingShared` and sends
//! commands back; only this thread touches `HostControl` and `paired.json`.

use std::net::SocketAddrV4;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use mixlink_core::config::NetConfig;
use mixlink_core::ids::DeviceId;
use mixlink_core::svc_ipc::{PairedInfo, PairingPrompt};
use mixlink_net::endpoint::{EndpointEvent, HostControl, HostControlConfig};
use mixlink_net::keys::StaticKeyPair;
use mixlink_net::pairing::{Now, PairedDevice};
use mixlink_net::{Transport, TransportConfig};
use mixlink_proto::channel::Channel;

use crate::error::SvcError;
use crate::pipe::session::PairingPort;
use crate::secrets;
use crate::stop::StopSignal;

/// Longest wait for a datagram, so stop, commands and timers are seen promptly.
const POLL: Duration = Duration::from_millis(50);

#[derive(Debug, Clone)]
pub struct SessionSettings {
    pub net: NetConfig,
    pub host: DeviceId,
    pub keys: StaticKeyPair,
    /// `secrets` folder, already secured.
    pub secrets: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionCommand {
    CancelPrompt,
    Remove(DeviceId),
}

/// What the pipe sees of pairing; updated by the session thread.
#[derive(Debug)]
pub struct PairingShared {
    prompt: Mutex<Option<PairingPrompt>>,
    paired: Mutex<Vec<PairedInfo>>,
    commands: Mutex<Sender<SessionCommand>>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl PairingShared {
    fn send(&self, command: SessionCommand) {
        // The thread is gone only while the service stops; the command no longer matters then.
        let _ = lock(&self.commands).send(command);
    }
}

impl PairingPort for PairingShared {
    fn prompt(&self) -> Option<PairingPrompt> {
        lock(&self.prompt).clone()
    }

    fn paired(&self) -> Vec<PairedInfo> {
        lock(&self.paired).clone()
    }

    fn cancel(&self) {
        self.send(SessionCommand::CancelPrompt);
    }

    fn remove(&self, device: DeviceId) {
        self.send(SessionCommand::Remove(device));
    }
}

fn now() -> Now {
    let unix_s = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs());
    Now {
        mono: Instant::now(),
        unix_s,
    }
}

fn infos(devices: &[PairedDevice]) -> Vec<PairedInfo> {
    devices
        .iter()
        .map(|known| PairedInfo {
            device: known.device,
            name: known.name.clone(),
            paired_at: known.paired_at,
        })
        .collect()
}

/// The running thread, what the pipe sees of it, and the bound session address.
#[derive(Debug)]
pub struct SessionListener {
    pub thread: JoinHandle<()>,
    pub shared: Arc<PairingShared>,
    pub addr: SocketAddrV4,
}

/// Opens the session port and starts the thread. An error (port taken) leaves the service
/// running without pairing.
pub fn start(
    stop: Arc<StopSignal>,
    settings: SessionSettings,
) -> Result<SessionListener, SvcError> {
    let (transport, mut lanes) = Transport::start(TransportConfig::host(&settings.net))
        .map_err(|error| SvcError::Session(error.to_string()))?;
    let missing = || SvcError::Session("control lane missing".to_owned());
    let tx = lanes.take_tx(Channel::Control).ok_or_else(missing)?;
    let rx = lanes.take_rx(Channel::Control).ok_or_else(missing)?;
    let paired = secrets::load_paired(&settings.secrets);
    let (commands, inbox) = mpsc::channel();
    let shared = Arc::new(PairingShared {
        prompt: Mutex::new(None),
        paired: Mutex::new(infos(&paired)),
        commands: Mutex::new(commands),
    });
    let addr = transport.local_addr();
    let control = HostControl::new(HostControlConfig {
        host: settings.host,
        keys: settings.keys,
        paired,
    });
    let worker = Worker {
        _transport: transport,
        tx,
        rx,
        control,
        inbox,
        shared: Arc::clone(&shared),
        secrets: settings.secrets,
    };
    let thread = thread::Builder::new()
        .name("svc-session".into())
        .spawn(move || worker.run(&stop))?;
    Ok(SessionListener {
        thread,
        shared,
        addr,
    })
}

struct Worker {
    _transport: Transport,
    tx: mixlink_net::TxLane,
    rx: mixlink_net::RxLane,
    control: HostControl,
    inbox: Receiver<SessionCommand>,
    shared: Arc<PairingShared>,
    secrets: PathBuf,
}

impl Worker {
    fn run(mut self, stop: &StopSignal) {
        while !stop.is_stopped() {
            let mut events = Vec::new();
            if let Some(mut dg) = self.rx.recv_timeout(POLL) {
                events = self.control.on_datagram(&mut self.tx, &mut dg, now());
            }
            while let Ok(command) = self.inbox.try_recv() {
                events.extend(self.command(command));
            }
            events.extend(self.control.tick(now()));
            for event in events {
                self.event(&event);
            }
            *lock(&self.shared.prompt) = self.control.prompt(now()).map(|view| PairingPrompt {
                device_name: view.device_name,
                pin: view.pin,
                expires_s: view.expires_s,
            });
        }
        log::info!("session listener stopped");
    }

    fn command(&mut self, command: SessionCommand) -> Vec<EndpointEvent> {
        match command {
            SessionCommand::CancelPrompt => {
                log::info!("pairing prompt cancelled from the tray");
                self.control.cancel_prompt(&mut self.tx);
                Vec::new()
            }
            SessionCommand::Remove(device) => {
                let mut paired = self.control.paired().to_vec();
                paired.retain(|known| known.device != device);
                let events = self.control.set_paired(paired);
                log::info!("removed paired device {device}");
                self.save();
                events
            }
        }
    }

    fn event(&mut self, event: &EndpointEvent) {
        match event {
            EndpointEvent::Paired(device) => {
                log::info!("paired with {} ({})", device.name, device.device);
                self.save();
            }
            // Streaming starts and stops the virtual monitor here (spec 0018 proves the session).
            EndpointEvent::SessionStarted {
                session,
                device,
                peer,
            } => log::info!("session {} started: {device} at {peer}", session.get()),
            EndpointEvent::SessionEnded { session, device } => {
                log::info!("session {} ended: {device}", session.get());
            }
        }
    }

    fn save(&self) {
        let paired = self.control.paired();
        if let Err(error) = secrets::save_paired(&self.secrets, paired) {
            log::error!("paired devices not saved: {error}");
        }
        *lock(&self.shared.paired) = infos(paired);
    }
}

#[cfg(test)]
#[path = "session_listen_tests.rs"]
mod tests;
