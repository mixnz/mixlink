//! User-visible text owned by Rust (ADR-0003: translated with the language task).
//! The window title lives in tauri.conf.json until the language task moves windows into code.

pub const TRAY_TOOLTIP: &str = "MixLink Host";
pub const TRAY_TOOLTIP_READY: &str = "MixLink Host: ready";
pub const TRAY_TOOLTIP_NOT_RUNNING: &str = "MixLink Host: service isn't running";
pub const TRAY_TOOLTIP_MISMATCH: &str = "MixLink Host: service version doesn't match";
pub const TRAY_TOOLTIP_UNTRUSTED: &str = "MixLink Host: can't verify the service";
pub const TRAY_OPEN: &str = "Open";
pub const TRAY_QUIT: &str = "Quit";
