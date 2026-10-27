//! Tray icon: the host's only always-visible UI.

use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{App, AppHandle, Manager};

use crate::strings;

const MAIN_WINDOW: &str = "main";
const TRAY_ID: &str = "main";
const MENU_OPEN: &str = "open";
const MENU_QUIT: &str = "quit";

pub fn build_tray(app: &App) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, MENU_OPEN, strings::TRAY_OPEN, true, None::<&str>)?;
    let quit = MenuItem::with_id(app, MENU_QUIT, strings::TRAY_QUIT, true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &quit])?;
    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip(strings::TRAY_TOOLTIP)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            MENU_OPEN => show_main_window(app),
            MENU_QUIT => app.exit(0),
            other => log::warn!("unknown tray menu item: {other}"),
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}

pub fn set_tooltip(app: &AppHandle, text: &str) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    if let Err(error) = tray.set_tooltip(Some(text)) {
        log::warn!("setting tray tooltip: {error}");
    }
}

pub fn is_main_window(label: &str) -> bool {
    label == MAIN_WINDOW
}

pub fn show_main_window(app: &AppHandle) {
    let Some(window) = app.get_webview_window(MAIN_WINDOW) else {
        log::warn!("host window missing");
        return;
    };
    if let Err(error) = window.show().and_then(|()| window.set_focus()) {
        log::warn!("showing host window: {error}");
    }
}
