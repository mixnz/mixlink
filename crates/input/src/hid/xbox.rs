//! Virtual Xbox pad input report 0x01 (spec 0016): the Xbox Series Bluetooth layout that the inbox
//! `xinputhid` filter and Steam read. Mirrored in `drivers/vhid/PadDescriptor.c`.
//!
//! Payload (16 B): LX LY RX RY u16 (0x8000 centre, +y down) @0..8, LT RT u16 0..=1023 @8..12,
//! hat u8 @12 (1..=8 clockwise from up, 0 centred), buttons u16 @13, share u8 @15 (bit 0).

use super::report::{PAYLOAD_MAX, Report, Target};

/// Pad slots 0..PAD_SLOTS: one virtual pad node each (split screen).
pub const PAD_SLOTS: u8 = 4;

/// Trigger logical maximum in the descriptor (10-bit).
const TRIGGER_MAX: u32 = 1023;

/// Button bits as the report carries them (HID button usage N = bit N-1).
pub mod buttons {
    pub const A: u16 = 1 << 0;
    pub const B: u16 = 1 << 1;
    pub const X: u16 = 1 << 3;
    pub const Y: u16 = 1 << 4;
    pub const LB: u16 = 1 << 6;
    pub const RB: u16 = 1 << 7;
    pub const VIEW: u16 = 1 << 10;
    pub const MENU: u16 = 1 << 11;
    pub const GUIDE: u16 = 1 << 12;
    pub const LS: u16 = 1 << 13;
    pub const RS: u16 = 1 << 14;
}

/// D-pad as the hat switch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dpad {
    Centered,
    Up,
    UpRight,
    Right,
    DownRight,
    Down,
    DownLeft,
    Left,
    UpLeft,
}

impl Dpad {
    const fn hat(self) -> u8 {
        match self {
            Dpad::Centered => 0,
            Dpad::Up => 1,
            Dpad::UpRight => 2,
            Dpad::Right => 3,
            Dpad::DownRight => 4,
            Dpad::Down => 5,
            Dpad::DownLeft => 6,
            Dpad::Left => 7,
            Dpad::UpLeft => 8,
        }
    }
}

/// Pad state. Sticks use the HID convention (+y = down); triggers span the full u16 and are scaled
/// to the report's 10 bits; `buttons` holds `buttons::*` bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PadState {
    pub buttons: u16,
    pub share: bool,
    pub dpad: Dpad,
    pub lx: i16,
    pub ly: i16,
    pub rx: i16,
    pub ry: i16,
    pub lt: u16,
    pub rt: u16,
}

impl PadState {
    pub const NEUTRAL: PadState = PadState {
        buttons: 0,
        share: false,
        dpad: Dpad::Centered,
        lx: 0,
        ly: 0,
        rx: 0,
        ry: 0,
        lt: 0,
        rt: 0,
    };

    /// Input report 0x01 of pad `slot`.
    // HOT PATH
    pub const fn report(&self, slot: u8) -> Report {
        let mut p = [0u8; PAYLOAD_MAX];
        let words = [
            stick(self.lx),
            stick(self.ly),
            stick(self.rx),
            stick(self.ry),
            trigger(self.lt),
            trigger(self.rt),
        ];
        let mut i = 0;
        while i < words.len() {
            let bytes = words[i].to_le_bytes();
            p[2 * i] = bytes[0];
            p[2 * i + 1] = bytes[1];
            i += 1;
        }
        p[12] = self.dpad.hat();
        let buttons = self.buttons.to_le_bytes();
        p[13] = buttons[0];
        p[14] = buttons[1];
        p[15] = self.share as u8;
        Report::new(Target::Pad(slot), p)
    }
}

/// i16 → 0..=65535 with 0 at 32768.
const fn stick(value: i16) -> u16 {
    value.cast_unsigned() ^ 0x8000
}

/// 0..=65535 → 0..=1023, rounded.
const fn trigger(value: u16) -> u16 {
    ((value as u32 * TRIGGER_MAX + 32_767) / 65_535) as u16
}
