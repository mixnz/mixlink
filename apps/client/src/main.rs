//! Client launcher: device list, pairing (spec 0018); spawns the stream window process.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod hosts;
mod pairing;
mod stream_proc;
mod stream_result;
mod strings;
mod window;

use std::process::ExitCode;

use tauri::{Manager, RunEvent};

use crate::hosts::HostsState;
use crate::pairing::PairingState;
use crate::stream_proc::StreamState;

fn main() -> ExitCode {
    env_logger::init();
    let app = tauri::Builder::default()
        .manage(StreamState::default())
        .manage(HostsState::default())
        .manage(PairingState::default())
        .invoke_handler(tauri::generate_handler![
            commands::start_stream,
            commands::last_stream_result,
            commands::list_hosts,
            commands::pair_request,
            commands::pair_submit,
            commands::pair_cancel,
            commands::forget_host
        ])
        .setup(|app| Ok(window::open_main_window(app.handle())?))
        .build(tauri::generate_context!());
    let app = match app {
        Ok(app) => app,
        Err(error) => {
            log::error!("launcher failed to start: {error}");
            return ExitCode::FAILURE;
        }
    };
    app.run(|handle, event| {
        // The launcher window is destroyed while streaming; that must not quit the app.
        if let RunEvent::ExitRequested {
            code: None, api, ..
        } = event
            && handle.state::<StreamState>().is_running()
        {
            api.prevent_exit();
        }
    });
    ExitCode::SUCCESS
}
