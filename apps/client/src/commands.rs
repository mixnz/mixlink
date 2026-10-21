//! Tauri commands called from `ui/src/api/`.

use tauri::{AppHandle, State};

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
