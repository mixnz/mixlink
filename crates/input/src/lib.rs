//! Client: capture keyboard, raw relative mouse, gamepad. Host: HID reports fed to `drivers/vhid`
//! (ADR-0007, spec 0016), SendInput fallback.

pub mod client;
pub mod hid;
pub mod keymap;
pub mod platform;
