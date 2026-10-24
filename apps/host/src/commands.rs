//! Tauri commands called from `ui/src/api/`.

use tauri::State;

use crate::service_link::{LinkState, SvcStatus};

#[tauri::command]
pub fn get_svc_status(state: State<'_, SvcStatus>) -> LinkState {
    state.get()
}
