//! Gamepad report (spec 0016): 16 buttons, hat, sticks left X/Y + right Z/Rz, triggers Rx/Ry,
//! all 16-bit in one INPUT item (layout D).

use super::report::{PAYLOAD_MAX, Report, Target};

/// Raw value of a released trigger. Not 0: SDL (and Steam) distrust an axis whose first value sits
/// at the very edge (≤ -32767 once scaled) and re-learn its rest the first time it passes near the
/// centre, so a trigger resting at 0 was taken as a centred axis (spec 0016). 2 / 65535 is far
/// inside any dead zone. Mirrored by the neutral gamepad report in `drivers/vhid/Feed.c`.
pub const TRIGGER_REST: u16 = 2;

/// D-pad as the HID hat switch: 0 = up, clockwise to 7 = up-left, 8 = centred (null state).
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
            Dpad::Up => 0,
            Dpad::UpRight => 1,
            Dpad::Right => 2,
            Dpad::DownRight => 3,
            Dpad::Down => 4,
            Dpad::DownLeft => 5,
            Dpad::Left => 6,
            Dpad::UpLeft => 7,
            Dpad::Centered => 8,
        }
    }
}

/// Xbox-style pad state. Sticks use the HID convention (+y = down); triggers span the full u16.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PadState {
    pub buttons: u16,
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
        dpad: Dpad::Centered,
        lx: 0,
        ly: 0,
        rx: 0,
        ry: 0,
        lt: 0,
        rt: 0,
    };

    // HOT PATH
    pub const fn report(&self) -> Report {
        let mut p = [0u8; PAYLOAD_MAX];
        let buttons = self.buttons.to_le_bytes();
        p[0] = buttons[0];
        p[1] = buttons[1];
        p[2] = self.dpad.hat();
        let axes = [
            stick(self.lx),
            stick(self.ly),
            stick(self.rx),
            stick(self.ry),
            trigger(self.lt),
            trigger(self.rt),
        ];
        let mut i = 0;
        while i < axes.len() {
            let bytes = axes[i].to_le_bytes();
            p[3 + 2 * i] = bytes[0];
            p[4 + 2 * i] = bytes[1];
            i += 1;
        }
        Report::new(Target::Gamepad, p)
    }
}

/// i16 → 0..=65535 with 0 at 32768.
const fn stick(value: i16) -> u16 {
    value.cast_unsigned() ^ 0x8000
}

/// 0..=65535 → `TRIGGER_REST`..=65535, rounded.
const fn trigger(value: u16) -> u16 {
    let span = (u16::MAX - TRIGGER_REST) as u32;
    let scaled = (value as u32 * span + 32_767) / 65_535;
    TRIGGER_REST + scaled as u16
}
