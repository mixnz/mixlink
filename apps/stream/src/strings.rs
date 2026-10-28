//! User-visible text of the stream window (ADR-0003: translated with the language task).

pub const WINDOW_TITLE: &str = "MixLink";
/// How hosts list this PC when `COMPUTERNAME` is missing.
pub const DEVICE_NAME: &str = "MixLink client";
pub const NO_PROFILE_FOLDER: &str =
    "Can't find your Windows profile folder, so this PC's pairing keys can't be read.";
/// Window title while keys and mouse go to the host (spec 0021).
pub const TITLE_CAPTURED: &str = "MixLink (Ctrl+Alt+Shift+Z frees your mouse and keyboard)";
