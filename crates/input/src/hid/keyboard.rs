//! NKRO keyboard report (spec 0016): modifier byte, bitmap of usages 0x04..=0x73, then 0x87..=0x8B.

use super::report::{PAYLOAD_MAX, Report, Target};

const MODIFIER_FIRST: u8 = 0xE0;
const MODIFIER_LAST: u8 = 0xE7;
const MAIN_FIRST: u8 = 0x04;
const MAIN_LAST: u8 = 0x73;
const INTL_FIRST: u8 = 0x87;
const INTL_LAST: u8 = 0x8B;
/// Byte holding usages 0x87..=0x8B (bits 0..=4).
const INTL_BYTE: usize = 15;

/// Keys held down, as the report payload itself.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct KeyboardState {
    payload: [u8; PAYLOAD_MAX],
}

impl KeyboardState {
    /// Presses or releases the key with HID usage `usage` (page 0x07). Returns false and changes
    /// nothing for a usage the report cannot carry.
    // HOT PATH
    pub fn set(&mut self, usage: u8, down: bool) -> bool {
        let Some((byte, bit)) = position(usage) else {
            return false;
        };
        if down {
            self.payload[byte] |= 1 << bit;
        } else {
            self.payload[byte] &= !(1 << bit);
        }
        true
    }

    pub fn clear(&mut self) {
        self.payload = [0; PAYLOAD_MAX];
    }

    // HOT PATH
    pub fn report(&self) -> Report {
        Report::new(Target::Keyboard, self.payload)
    }
}

/// Byte and bit of `usage` in the payload.
fn position(usage: u8) -> Option<(usize, u8)> {
    match usage {
        MODIFIER_FIRST..=MODIFIER_LAST => Some((0, usage - MODIFIER_FIRST)),
        MAIN_FIRST..=MAIN_LAST => {
            let index = usize::from(usage - MAIN_FIRST);
            Some((1 + index / 8, (usage - MAIN_FIRST) % 8))
        }
        INTL_FIRST..=INTL_LAST => Some((INTL_BYTE, usage - INTL_FIRST)),
        _ => None,
    }
}
