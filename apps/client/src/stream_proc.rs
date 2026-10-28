//! Spawns and supervises the `mixlink-stream` child process.
//! One supervisor thread per stream reads the child's stdout until EOF, then reports the result.

use std::io::{self, BufRead, BufReader};
use std::net::{IpAddr, SocketAddrV4};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, PoisonError};
use std::time::Instant;
use std::{env, thread};

use mixlink_core::ids::DeviceId;
use mixlink_core::ipc::{CloseReason, IPC_VERSION, LatencySummary, StreamToLauncher, decode_line};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::stream_result::{StreamResult, classify};
use crate::window;

const STREAM_EXE: &str = "mixlink-stream.exe";
/// Event sent to a launcher window that is still open when the stream ends (it died before `Ready`).
pub const STREAM_RESULT_EVENT: &str = "stream-result";

/// The host a stream connects to (spec 0020).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectTarget {
    pub host: DeviceId,
    pub addr: SocketAddrV4,
}

/// The session port of a discovered host; `None` for IPv6 (the session is IPv4 only).
pub fn target_of(host: DeviceId, address: IpAddr, port: u16) -> Option<ConnectTarget> {
    match address {
        IpAddr::V4(ip) => Some(ConnectTarget {
            host,
            addr: SocketAddrV4::new(ip, port),
        }),
        IpAddr::V6(_) => None,
    }
}

/// `mixlink-stream` arguments for a target.
pub fn connect_args(target: &ConnectTarget) -> Vec<String> {
    vec![
        "--connect".to_owned(),
        target.host.to_string(),
        "--addr".to_owned(),
        target.addr.to_string(),
    ]
}

/// The last session's per-stage latency, shown under its host row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LastLatency {
    pub host_id: String,
    pub latency: LatencySummary,
}

#[derive(Debug, Default)]
pub struct StreamState {
    pub(crate) running: AtomicBool,
    last_result: Mutex<Option<StreamResult>>,
    last_latency: Mutex<Option<LastLatency>>,
}

impl StreamState {
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    pub fn last_result(&self) -> Option<StreamResult> {
        self.last_result
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    pub fn last_latency(&self) -> Option<LastLatency> {
        self.last_latency
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    pub fn keep_latency(&self, host_id: &str, latency: LatencySummary) {
        *self
            .last_latency
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(LastLatency {
            host_id: host_id.to_owned(),
            latency,
        });
    }

    fn finish(&self, result: StreamResult) {
        *self
            .last_result
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(result);
        self.running.store(false, Ordering::SeqCst);
    }
}

#[derive(Debug, thiserror::Error)]
pub enum StartError {
    #[error("a stream is already running")]
    AlreadyRunning,
    #[error("{0}")]
    Spawn(#[source] io::Error),
}

/// Claims the single stream slot.
pub fn try_begin(state: &StreamState) -> Result<(), StartError> {
    state
        .running
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .map(|_| ())
        .map_err(|_| StartError::AlreadyRunning)
}

/// `mixlink-stream.exe` next to the launcher (`target/<profile>/` or `dist/`).
pub fn stream_exe_path() -> io::Result<PathBuf> {
    let exe = env::current_exe()?;
    let dir = exe
        .parent()
        .ok_or_else(|| io::Error::other("launcher path has no parent"))?;
    Ok(dir.join(STREAM_EXE))
}

pub fn spawn_at(path: &Path, args: &[String]) -> Result<Child, StartError> {
    Command::new(path)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(StartError::Spawn)
}

/// Starts one stream to `target`. On failure the returned result is also stored as the last result.
pub fn start(app: &AppHandle, target: &ConnectTarget) -> Result<(), StreamResult> {
    let state = app.state::<StreamState>();
    try_begin(&state).map_err(|error| StreamResult::StreamError {
        message: error.to_string(),
    })?;
    let started = Instant::now();
    let child = stream_exe_path()
        .map_err(StartError::Spawn)
        .and_then(|path| spawn_at(&path, &connect_args(target)));
    let child = match child {
        Ok(child) => child,
        Err(error) => {
            let result = StreamResult::SpawnFailed {
                message: error.to_string(),
            };
            state.finish(result.clone());
            return Err(result);
        }
    };
    let pid = child.id();
    let handle = app.clone();
    let host_id = target.host.to_string();
    let supervisor = thread::Builder::new()
        .name("stream-supervisor".into())
        .spawn(move || supervise(&handle, child, started, &host_id));
    if let Err(error) = supervisor {
        log::error!("stream supervisor thread: {error}");
        let result = StreamResult::StreamError {
            message: error.to_string(),
        };
        state.finish(result.clone());
        return Err(result);
    }
    log::info!("stream started (pid {pid})");
    Ok(())
}

fn supervise(app: &AppHandle, mut child: Child, started: Instant, host_id: &str) {
    // Held until the child exits: dropping it is the "launcher gone" signal (stdin EOF).
    let _stdin = child.stdin.take();
    let mut ready = None;
    let mut closed: Option<CloseReason> = None;
    if let Some(stdout) = child.stdout.take() {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            match decode_line::<StreamToLauncher>(&line) {
                Ok(StreamToLauncher::Ready { ipc_version }) => {
                    ready = Some(ipc_version);
                    on_ready(app, &mut child, ipc_version, started);
                }
                Ok(StreamToLauncher::Closed { reason, latency }) => {
                    if let Some(latency) = latency {
                        app.state::<StreamState>().keep_latency(host_id, latency);
                    }
                    closed = Some(reason);
                }
                Ok(StreamToLauncher::Connecting) => log::info!("stream connecting"),
                Ok(StreamToLauncher::Connected {
                    width,
                    height,
                    codec,
                    ..
                }) => {
                    log::info!("stream connected: {width}x{height} {codec}");
                }
                Err(error) => log::warn!("ignoring stream line: {error}"),
            }
        }
    }
    let code = match child.wait() {
        Ok(status) => status.code(),
        Err(error) => {
            log::warn!("waiting for stream: {error}");
            None
        }
    };
    let result = classify(ready, closed, code);
    log::info!("stream ended: {result:?}");
    app.state::<StreamState>().finish(result.clone());
    report(app, &result);
}

fn on_ready(app: &AppHandle, child: &mut Child, ipc_version: u32, started: Instant) {
    if ipc_version != IPC_VERSION {
        log::error!("stream IPC v{ipc_version}, launcher v{IPC_VERSION}");
        if let Err(error) = child.kill() {
            log::warn!("killing stream: {error}");
        }
        return;
    }
    log::info!("stream ready after {} ms", started.elapsed().as_millis());
    // ADR-0001: no webview stays open while streaming.
    if let Some(main) = app.get_webview_window(window::MAIN_WINDOW)
        && let Err(error) = main.destroy()
    {
        log::warn!("closing launcher window: {error}");
    }
}

/// Shows the result: in the still-open window, or in a recreated one.
fn report(app: &AppHandle, result: &StreamResult) {
    if app.get_webview_window(window::MAIN_WINDOW).is_some() {
        if let Err(error) = app.emit(STREAM_RESULT_EVENT, result) {
            log::warn!("emitting stream result: {error}");
        }
    } else if let Err(error) = window::open_main_window(app) {
        log::error!("reopening launcher window: {error}");
    }
}

#[cfg(test)]
#[path = "stream_proc_tests.rs"]
mod tests;
