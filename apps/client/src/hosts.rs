//! Hosts found on the LAN (spec 0017): runs the discovery `Browser` while the launcher window is
//! open and sends each new list to the UI.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use mixlink_net::discovery::{Browser, BrowserConfig, HostEntry};
use mixlink_proto::header::WIRE_VERSION;
use serde::Serialize;
use tauri::{AppHandle, Emitter};

pub const HOSTS_EVENT: &str = "hosts-changed";

/// One host as the UI shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostRow {
    pub id: String,
    pub name: String,
    pub address: String,
    pub port: u16,
    /// Same wire protocol version as this launcher.
    pub compatible: bool,
}

pub fn rows(entries: &[HostEntry]) -> Vec<HostRow> {
    entries
        .iter()
        .map(|entry| HostRow {
            id: entry.id.to_string(),
            name: entry.name.clone(),
            address: entry.address.to_string(),
            port: entry.port,
            compatible: entry.ver == WIRE_VERSION,
        })
        .collect()
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[derive(Debug, Default)]
pub struct HostsState {
    browser: Mutex<Option<Browser>>,
    rows: Arc<Mutex<Vec<HostRow>>>,
}

impl HostsState {
    /// Starts browsing; does nothing when already running. A failure leaves the list empty.
    pub fn start(&self, app: &AppHandle) {
        let mut browser = lock(&self.browser);
        if browser.is_some() {
            return;
        }
        let shared = Arc::clone(&self.rows);
        let app = app.clone();
        let on_change = Box::new(move |entries: Vec<HostEntry>| {
            let rows = rows(&entries);
            *lock(&shared) = rows.clone();
            if let Err(error) = app.emit(HOSTS_EVENT, &rows) {
                log::warn!("emitting host list: {error}");
            }
        });
        match Browser::start(BrowserConfig::default(), on_change) {
            Ok(started) => *browser = Some(started),
            Err(error) => log::warn!("LAN discovery not started: {error}"),
        }
    }

    /// Stops browsing and forgets the list; the next `start` finds hosts again.
    pub fn stop(&self) {
        let browser = lock(&self.browser).take();
        drop(browser);
        lock(&self.rows).clear();
    }

    pub fn current(&self) -> Vec<HostRow> {
        lock(&self.rows).clone()
    }
}

#[cfg(test)]
#[path = "hosts_tests.rs"]
mod tests;
