//! Host tray app: control UI for the `mixlink-svc` service (ADR-0005): status, pairing PIN, paired devices.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod pairing;
mod platform;
mod service_link;
mod strings;
mod tray;

use std::process::ExitCode;

use tauri::WindowEvent;

use crate::service_link::SvcStatus;

fn main() -> ExitCode {
    env_logger::init();
    let result = tauri::Builder::default()
        .manage(SvcStatus::default())
        .invoke_handler(tauri::generate_handler![
            commands::get_svc_status,
            commands::cancel_pairing,
            commands::list_paired,
            commands::remove_paired
        ])
        .setup(|app| {
            tray::build_tray(app)?;
            service_link::spawn_link(app.handle().clone())?;
            Ok(())
        })
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
