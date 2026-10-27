//! Tauri commands called from `ui/src/api/`.

use tauri::{AppHandle, State};

use crate::hosts::{HostRow, HostsState};
use crate::pairing::{self, PairingState};
use crate::stream_proc::{self, StreamState};
use crate::stream_result::StreamResult;

#[tauri::command]
pub fn start_stream(app: AppHandle) -> Result<(), StreamResult> {
    stream_proc::start(&app)
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
