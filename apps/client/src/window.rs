//! The launcher window. Built in code (not tauri.conf.json) because it is recreated after a stream.

use tauri::{AppHandle, WebviewUrl, WebviewWindowBuilder};

use crate::strings;

pub const MAIN_WINDOW: &str = "main";

pub fn open_main_window(app: &AppHandle) -> tauri::Result<()> {
    WebviewWindowBuilder::new(app, MAIN_WINDOW, WebviewUrl::default())
        .title(strings::WINDOW_TITLE)
        .inner_size(900.0, 600.0)
        .build()?;
    Ok(())
}
