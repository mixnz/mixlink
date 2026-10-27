//! Per-connection message handling: pure, so the pipe server only moves bytes.
//! Input comes from any interactive user while we run as SYSTEM: anything unexpected closes.

use std::time::Duration;

use mixlink_core::ids::DeviceId;
use mixlink_core::ipc::decode_line;
use mixlink_core::svc_ipc::{
    PIPE_VERSION, PairedInfo, PairingPrompt, SvcState, SvcToTray, TrayToSvc,
};

/// The reply to send, and whether to close the connection after it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reply {
    pub msg: SvcToTray,
    pub close: bool,
}

/// What the pipe may see and do of pairing (spec 0018). Never exposes keys.
pub trait PairingPort {
    fn prompt(&self) -> Option<PairingPrompt>;
    fn paired(&self) -> Vec<PairedInfo>;
    fn cancel(&self);
    fn remove(&self, device: DeviceId);
}

/// Pairing is off (no session listener): nothing to show, requests do nothing.
#[derive(Debug, Clone, Copy)]
pub struct NoPairing;

impl PairingPort for NoPairing {
    fn prompt(&self) -> Option<PairingPrompt> {
        None
    }

    fn paired(&self) -> Vec<PairedInfo> {
        Vec::new()
    }

    fn cancel(&self) {}

    fn remove(&self, _device: DeviceId) {}
}

#[derive(Debug, Default)]
pub struct Session {
    greeted: bool,
}

impl Session {
    pub fn new() -> Session {
        Session::default()
    }

    /// Handles one received line (with or without its `\n`). `network_public` runs only for
    /// `GetStatus`.
    pub fn handle_line(
        &mut self,
        line: &[u8],
        uptime: Duration,
        network_public: impl FnOnce() -> bool,
        pairing: &dyn PairingPort,
    ) -> Reply {
        let Ok(text) = std::str::from_utf8(line) else {
            return refuse("line is not UTF-8");
        };
        let msg = match decode_line::<TrayToSvc>(text) {
            Ok(msg) => msg,
            Err(error) => {
                log::debug!("bad pipe message: {error}");
                return refuse("message not understood");
            }
        };
        if !self.greeted {
            return match msg {
                TrayToSvc::Hello { pipe_version } => {
                    self.greeted = true;
                    Reply {
                        msg: SvcToTray::Hello {
                            pipe_version: PIPE_VERSION,
                            svc_version: env!("CARGO_PKG_VERSION").to_owned(),
                        },
                        close: pipe_version != PIPE_VERSION,
                    }
                }
                _ => refuse("Hello must come first"),
            };
        }
        let msg = match msg {
            TrayToSvc::Hello { .. } => return refuse("Hello already received"),
            TrayToSvc::GetStatus => SvcToTray::Status {
                state: SvcState::Idle,
                uptime_s: uptime.as_secs(),
                network_public: network_public(),
                pairing: pairing.prompt(),
            },
            TrayToSvc::CancelPairing => {
                pairing.cancel();
                SvcToTray::Done
            }
            TrayToSvc::ListPaired => SvcToTray::Paired {
                devices: pairing.paired(),
            },
            TrayToSvc::RemovePaired { device } => {
                pairing.remove(device);
                SvcToTray::Done
            }
        };
        Reply { msg, close: false }
    }
}

/// Reply for a line longer than `svc_ipc::MAX_LINE`.
pub fn too_long_reply() -> Reply {
    refuse("line too long")
}

fn refuse(message: &str) -> Reply {
    Reply {
        msg: SvcToTray::Error {
            message: message.to_owned(),
        },
        close: true,
    }
}

#[cfg(test)]
#[path = "session_tests.rs"]
mod tests;
