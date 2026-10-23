//! Device-level input events and their mapping to each backend.

use crate::report::PadState;

/// One input event, independent of the backend that delivers it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Input {
    /// HID keyboard usage.
    Key {
        usage: u8,
        down: bool,
    },
    MouseMove {
        dx: i16,
        dy: i16,
    },
    /// Descriptor range 0..32767.
    MouseAbs {
        x: u16,
        y: u16,
    },
    Pad(PadState),
}

/// Held keys of the boot-layout keyboard report (6KRO).
#[derive(Clone, Copy, Debug, Default)]
pub struct KeyboardState {
    held: [u8; 6],
}

impl KeyboardState {
    /// Applies one press / release and returns the report payload. HOT PATH
    pub fn apply(&mut self, usage: u8, down: bool) -> [u8; 8] {
        let pos = self.held.iter().position(|&k| k == usage && k != 0);
        match (down, pos) {
            (true, None) => {
                if let Some(slot) = self.held.iter_mut().find(|k| **k == 0) {
                    *slot = usage;
                }
            }
            (false, Some(p)) => {
                self.held.copy_within(p + 1.., p);
                self.held[5] = 0;
            }
            (true, Some(_)) | (false, None) => {}
        }
        crate::report::kbd_payload(0, &self.held)
    }
}

/// Virtual-key code for the HID usages the harness sends. Enter is deliberately absent.
pub fn usage_to_vk(usage: u8) -> Option<u16> {
    match usage {
        0x68..=0x73 => Some(0x7C + u16::from(usage - 0x68)), // F13..F24
        0x04..=0x1D => Some(0x41 + u16::from(usage - 0x04)), // A..Z
        0x29 => Some(0x1B),                                  // Esc
        0x2B => Some(0x09),                                  // Tab
        0x2C => Some(0x20),                                  // Space
        _ => None,
    }
}

/// XInput gamepad fields (same layout as `XINPUT_GAMEPAD`), backend-neutral.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct XinputPad {
    pub buttons: u16,
    pub left_trigger: u8,
    pub right_trigger: u8,
    pub thumb_lx: i16,
    pub thumb_ly: i16,
    pub thumb_rx: i16,
    pub thumb_ry: i16,
}

/// HID button bit (Xbox order A, B, X, Y, LB, RB, Back, Start, LS, RS, Guide) -> XInput flag.
/// Bit 11 (Share) has no XInput flag.
const XINPUT_BUTTONS: [u16; 11] = [
    0x1000, 0x2000, 0x4000, 0x8000, 0x0100, 0x0200, 0x0020, 0x0010, 0x0040, 0x0080, 0x0400,
];
/// Hat 0..7 clockwise from up -> D-pad flags (up 1, down 2, left 4, right 8).
const XINPUT_HAT: [u16; 8] = [
    0x1,
    0x1 | 0x8,
    0x8,
    0x2 | 0x8,
    0x2,
    0x2 | 0x4,
    0x4,
    0x1 | 0x4,
];

fn axis_x(v: u16) -> i16 {
    (v ^ 0x8000) as i16
}

/// HID Y grows downward, XInput Y grows upward; center maps to 0.
fn axis_y(v: u16) -> i16 {
    (0x8000 - i32::from(v)).clamp(-32767, 32767) as i16
}

// HOT PATH
pub fn pad_to_xinput(state: &PadState) -> XinputPad {
    let mut buttons = 0u16;
    for (bit, flag) in XINPUT_BUTTONS.iter().enumerate() {
        if state.buttons & (1 << bit) != 0 {
            buttons |= flag;
        }
    }
    if let Some(flags) = XINPUT_HAT.get(usize::from(state.hat)) {
        buttons |= flags;
    }
    XinputPad {
        buttons,
        left_trigger: state.lt,
        right_trigger: state.rt,
        thumb_lx: axis_x(state.lx),
        thumb_ly: axis_y(state.ly),
        thumb_rx: axis_x(state.rx),
        thumb_ry: axis_y(state.ry),
    }
}

#[cfg(test)]
#[path = "input_tests.rs"]
mod tests;
