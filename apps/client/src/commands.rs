//! Tauri commands called from `ui/src/api/`.

use mixlink_core::ids::DeviceId;
use tauri::{AppHandle, State};

use crate::hosts::{HostRow, HostsState};
use crate::pairing::{self, PairingState};
use crate::stream_proc::{self, LastLatency, StreamState, target_of};
use crate::stream_result::StreamResult;

/// Connects to a paired host found on the LAN (spec 0020).
#[tauri::command]
pub fn start_stream(
    app: AppHandle,
    hosts: State<'_, HostsState>,
    host_id: String,
) -> Result<(), StreamResult> {
    let gone = || StreamResult::HostUnreachable;
    let id: DeviceId = host_id.parse().map_err(|_| gone())?;
    let entry = hosts.find(&id).ok_or_else(gone)?;
    let target = target_of(id, entry.address, entry.port).ok_or_else(gone)?;
    stream_proc::start(&app, &target)
}

#[tauri::command]
pub fn last_latency(state: State<'_, StreamState>) -> Option<LastLatency> {
    state.last_latency()
}

#[tauri::command]
pub fn last_stream_result(state: State<'_, StreamState>) -> Option<StreamResult> {
    state.last_result()
}

#[tauri::command]
pub fn list_hosts(state: State<'_, HostsState>) -> Vec<HostRow> {
    state.current()
}

#[tauri::command]
pub fn pair_request(
    app: AppHandle,
    state: State<'_, PairingState>,
    id: String,
) -> Result<(), String> {
    state.request(&app, &id)
}

#[tauri::command]
pub fn pair_submit(state: State<'_, PairingState>, pin: String) -> Result<(), String> {
    state.submit(&pin)
}

#[tauri::command]
pub fn pair_cancel(app: AppHandle, state: State<'_, PairingState>) {
    state.cancel(&app);
}

#[tauri::command]
pub fn forget_host(app: AppHandle, id: String) -> Result<(), String> {
    pairing::forget(&app, &id)
}
