//! Keeps a pipe connection to `mixlink-svc` (spec 0012) and reports its state to the UI and tray.

use std::sync::{Mutex, PoisonError};
use std::thread;
use std::time::Duration;

use mixlink_core::svc_ipc::{PIPE_NAME, SvcPipe, SvcPipeError};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::pairing::{PromptState, same_prompt, should_show, trusted};
use crate::strings;
use crate::tray;

pub const SVC_STATUS_EVENT: &str = "svc-status";
const POLL: Duration = Duration::from_secs(1);
const RETRY: Duration = Duration::from_secs(2);

/// What the UI shows about the service.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum LinkState {
    Connected {
        svc_version: String,
        uptime_s: u64,
        network_public: bool,
        /// A client is pairing: show this PIN (spec 0018).
        pairing: Option<PromptState>,
    },
    NotRunning,
    VersionMismatch {
        svc_version: String,
    },
    /// The pipe is served by another program, not the MixLink service: nothing is shown from it.
    Untrusted,
}

/// Decides when a new state is worth an event: uptime ticking alone is not.
#[derive(Debug, Default)]
pub struct LinkTracker {
    last: Option<LinkState>,
}

impl LinkTracker {
    pub fn update(&mut self, next: LinkState) -> Option<LinkState> {
        let changed = match (&self.last, &next) {
            (
                Some(LinkState::Connected {
                    svc_version: a,
                    network_public: pa,
                    pairing: qa,
                    ..
                }),
                LinkState::Connected {
                    svc_version: b,
                    network_public: pb,
                    pairing: qb,
                    ..
                },
            ) => a != b || pa != pb || !same_prompt(qa.as_ref(), qb.as_ref()),
            (Some(last), _) => *last != next,
            (None, _) => true,
        };
        self.last = Some(next.clone());
        changed.then_some(next)
    }
}

pub fn state_from_error(error: &SvcPipeError) -> LinkState {
    match error {
        SvcPipeError::VersionMismatch { svc_version, .. } => LinkState::VersionMismatch {
            svc_version: svc_version.clone(),
        },
        SvcPipeError::Untrusted => LinkState::Untrusted,
        SvcPipeError::NotRunning
        | SvcPipeError::Io(_)
        | SvcPipeError::Ipc(_)
        | SvcPipeError::Unexpected => LinkState::NotRunning,
    }
}

/// Latest state, for a window that opens after the last event.
#[derive(Debug)]
pub struct SvcStatus(Mutex<LinkState>);

impl Default for SvcStatus {
    fn default() -> SvcStatus {
        SvcStatus(Mutex::new(LinkState::NotRunning))
    }
}

impl SvcStatus {
    pub fn get(&self) -> LinkState {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn set(&self, state: LinkState) {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner) = state;
    }
}

/// Starts the `host-svc-link` thread; it lives as long as the app.
pub fn spawn_link(app: AppHandle) -> std::io::Result<()> {
    thread::Builder::new()
        .name("host-svc-link".into())
        .spawn(move || link_loop(&app))
        .map(drop)
}

fn link_loop(app: &AppHandle) {
    let mut tracker = LinkTracker::default();
    loop {
        // The server is checked before anything is sent: a fake one may never answer `Hello`.
        match SvcPipe::connect_checked(PIPE_NAME, trusted) {
            Ok((mut pipe, svc_version)) => {
                let mut shown: Option<PromptState> = None;
                loop {
                    match pipe.status() {
                        Ok(report) => {
                            let prompt = report.pairing.map(PromptState::from);
                            if should_show(shown.as_ref(), prompt.as_ref()) {
                                tray::show_main_window(app);
                            }
                            shown.clone_from(&prompt);
                            let state = LinkState::Connected {
                                svc_version: svc_version.clone(),
                                uptime_s: report.uptime_s,
                                network_public: report.network_public,
                                pairing: prompt,
                            };
                            publish(app, &mut tracker, state);
                        }
                        Err(error) => {
                            log::info!("service link lost: {error}");
                            publish(app, &mut tracker, state_from_error(&error));
                            break;
                        }
                    }
                    thread::sleep(POLL);
                }
            }
            Err(SvcPipeError::Untrusted) => {
                log::warn!("service pipe is not served by the MixLink service");
                publish(app, &mut tracker, LinkState::Untrusted);
            }
            Err(error) => {
                log::debug!("service link: {error}");
                publish(app, &mut tracker, state_from_error(&error));
            }
        }
        thread::sleep(RETRY);
    }
}

fn publish(app: &AppHandle, tracker: &mut LinkTracker, state: LinkState) {
    app.state::<SvcStatus>().set(state.clone());
    let Some(changed) = tracker.update(state) else {
        return;
    };
    tray::set_tooltip(app, tooltip(&changed));
    if let Err(error) = app.emit(SVC_STATUS_EVENT, &changed) {
        log::warn!("emitting service status: {error}");
    }
}

fn tooltip(state: &LinkState) -> &'static str {
    match state {
        LinkState::Connected { .. } => strings::TRAY_TOOLTIP_READY,
        LinkState::NotRunning => strings::TRAY_TOOLTIP_NOT_RUNNING,
        LinkState::VersionMismatch { .. } => strings::TRAY_TOOLTIP_MISMATCH,
        LinkState::Untrusted => strings::TRAY_TOOLTIP_UNTRUSTED,
    }
}

#[cfg(test)]
#[path = "service_link_tests.rs"]
mod tests;
