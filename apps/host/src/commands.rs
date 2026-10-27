//! Tauri commands called from `ui/src/api/`.

use tauri::State;

use crate::pairing::{self, PairedRow, TrayPairError};
use crate::service_link::{LinkState, SvcStatus};

#[tauri::command]
pub fn get_svc_status(state: State<'_, SvcStatus>) -> LinkState {
    state.get()
}

#[tauri::command(async)]
pub fn cancel_pairing() -> Result<(), TrayPairError> {
    pairing::cancel()
}

#[tauri::command(async)]
pub fn list_paired() -> Result<Vec<PairedRow>, TrayPairError> {
    pairing::list()
}

#[tauri::command(async)]
pub fn remove_paired(device: String) -> Result<(), TrayPairError> {
    pairing::remove(&device)
}
