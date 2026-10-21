//! Host tray app: PIN display and status (later); wires capture -> codec -> net (later).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod strings;
mod tray;

use std::process::ExitCode;

use tauri::WindowEvent;

fn main() -> ExitCode {
    env_logger::init();
    let result = tauri::Builder::default()
        .setup(|app| Ok(tray::build_tray(app)?))
        .on_window_event(|window, event| {
            // Closing the window keeps the host running in the tray.
            if let WindowEvent::CloseRequested { api, .. } = event
                && tray::is_main_window(window.label())
            {
                api.prevent_close();
                if let Err(error) = window.hide() {
                    log::warn!("hiding host window: {error}");
                }
            }
        })
        .run(tauri::generate_context!());
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            log::error!("host failed: {error}");
            ExitCode::FAILURE
        }
    }
}
