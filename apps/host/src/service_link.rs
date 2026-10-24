//! Keeps a pipe connection to `mixlink-svc` (spec 0012) and reports its state to the UI and tray.

use std::sync::{Mutex, PoisonError};
use std::thread;
use std::time::Duration;

use mixlink_core::svc_ipc::{PIPE_NAME, SvcPipe, SvcPipeError};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

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
    Connected { svc_version: String, uptime_s: u64 },
    NotRunning,
    VersionMismatch { svc_version: String },
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
                Some(LinkState::Connected { svc_version: a, .. }),
                LinkState::Connected { svc_version: b, .. },
            ) => a != b,
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
        match SvcPipe::connect(PIPE_NAME) {
            Ok((mut pipe, svc_version)) => {
                publish(app, &mut tracker, connected(&svc_version, 0));
                loop {
                    match pipe.status() {
                        Ok((_, uptime_s)) => {
                            publish(app, &mut tracker, connected(&svc_version, uptime_s));
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
            Err(error) => {
                log::debug!("service link: {error}");
                publish(app, &mut tracker, state_from_error(&error));
            }
        }
        thread::sleep(RETRY);
    }
}

fn connected(svc_version: &str, uptime_s: u64) -> LinkState {
    LinkState::Connected {
        svc_version: svc_version.to_owned(),
        uptime_s,
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
    }
}

#[cfg(test)]
#[path = "service_link_tests.rs"]
mod tests;
