//! One input report for one `vhid` collection: target report ID + payload (spec 0016).

use super::gamepad::PadState;

/// Largest payload of any collection (keyboard); `PAYLOAD_MAX` in `drivers/vhid/Driver.h`.
pub const PAYLOAD_MAX: usize = 16;

/// `vhid` collection, by its report ID (`drivers/vhid/Descriptor.c`).
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Keyboard = 1,
    MouseRel = 2,
    MouseAbs = 3,
    Gamepad = 4,
}

impl Target {
    pub const fn payload_len(self) -> usize {
        match self {
            Target::Keyboard => 16,
            Target::MouseRel => 7,
            Target::MouseAbs => 5,
            Target::Gamepad => 15,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Report {
    target: Target,
    /// Zero after `target.payload_len()`.
    payload: [u8; PAYLOAD_MAX],
}

impl Report {
    /// `payload` beyond `target.payload_len()` must be zero (builders keep it so).
    pub(crate) const fn new(target: Target, payload: [u8; PAYLOAD_MAX]) -> Report {
        Report { target, payload }
    }

    pub fn target(&self) -> Target {
        self.target
    }

    pub fn payload(&self) -> &[u8] {
        &self.payload[..self.target.payload_len()]
    }
}

/// What the driver feeds when the feeder closes (release-all): no keys, no buttons, pad centred.
/// Absolute mouse is left out so the cursor does not jump.
pub fn neutral_reports() -> [Report; 3] {
    [
        Report::new(Target::Keyboard, [0; PAYLOAD_MAX]),
        Report::new(Target::MouseRel, [0; PAYLOAD_MAX]),
        PadState::NEUTRAL.report(),
    ]
}
