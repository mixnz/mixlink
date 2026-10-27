//! The launcher window. Built in code (not tauri.conf.json) because it is recreated after a stream.
//! LAN discovery runs while it is open (spec 0017).

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder, WindowEvent};

use crate::hosts::HostsState;
use crate::pairing;
use crate::strings;

pub const MAIN_WINDOW: &str = "main";

pub fn open_main_window(app: &AppHandle) -> tauri::Result<()> {
    let window = WebviewWindowBuilder::new(app, MAIN_WINDOW, WebviewUrl::default())
        .title(strings::WINDOW_TITLE)
        .inner_size(900.0, 600.0)
        .build()?;
    let handle = app.clone();
    window.on_window_event(move |event| {
        if let WindowEvent::Destroyed = event {
            handle.state::<HostsState>().stop();
        }
    });
    let hosts = app.state::<HostsState>();
    hosts.set_known(app, pairing::known_ids());
    hosts.start(app);
    Ok(())
}
