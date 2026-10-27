//! Per-connection message handling: pure, so the pipe server only moves bytes.
//! Input comes from any interactive user while we run as SYSTEM: anything unexpected closes.

use std::time::Duration;

use mixlink_core::ipc::decode_line;
use mixlink_core::svc_ipc::{PIPE_VERSION, SvcState, SvcToTray, TrayToSvc};

/// The reply to send, and whether to close the connection after it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reply {
    pub msg: SvcToTray,
    pub close: bool,
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
        match (self.greeted, msg) {
            (false, TrayToSvc::Hello { pipe_version }) => {
                self.greeted = true;
                Reply {
                    msg: SvcToTray::Hello {
                        pipe_version: PIPE_VERSION,
                        svc_version: env!("CARGO_PKG_VERSION").to_owned(),
                    },
                    close: pipe_version != PIPE_VERSION,
                }
            }
            (false, TrayToSvc::GetStatus) => refuse("Hello must come first"),
            (true, TrayToSvc::Hello { .. }) => refuse("Hello already received"),
            (true, TrayToSvc::GetStatus) => Reply {
                msg: SvcToTray::Status {
                    state: SvcState::Idle,
                    uptime_s: uptime.as_secs(),
                    network_public: network_public(),
                },
                close: false,
            },
        }
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
