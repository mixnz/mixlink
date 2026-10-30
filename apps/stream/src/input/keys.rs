//! Keys from the stream window (spec 0021): winit's scan code (set 1, `0xE0..` = extended) as the
//! hook would see it, so both paths give the same HID usage. Used when the hook does not get the key.

use mixlink_input::platform::windows::kbd_hook::RawKey;
use winit::keyboard::KeyCode;

/// `VK_PAUSE` / `VK_NUMLOCK`: their scan codes are ambiguous, `keymap::usage` decides by the vk.
const VK_PAUSE: u16 = 0x13;
const VK_NUMLOCK: u16 = 0x90;

// HOT PATH
pub fn raw_key(code: KeyCode, scancode: u32, down: bool) -> RawKey {
    let vk = match code {
        KeyCode::Pause => VK_PAUSE,
        KeyCode::NumLock => VK_NUMLOCK,
        _ => 0,
    };
    RawKey {
        scan: (scancode & 0xFF) as u16,
        extended: scancode & 0xFF00 == 0xE000,
        vk,
        down,
    }
}

#[cfg(test)]
#[path = "keys_tests.rs"]
mod tests;
