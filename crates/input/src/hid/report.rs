//! One input report for one `vhid` collection: target + payload (spec 0016).

use super::xbox::{PAD_SLOTS, PadState};

/// Largest payload of any collection (keyboard, pad); `PAYLOAD_MAX` in `drivers/vhid/Driver.h`.
pub const PAYLOAD_MAX: usize = 16;

/// Where a report goes: a collection of the keyboard / mouse node, or pad `slot`'s input report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Keyboard,
    MouseRel,
    MouseAbs,
    Pad(u8),
}

impl Target {
    /// Target byte of the `FEED` IOCTL: report IDs 1..=3, pads 0x10 + slot.
    pub const fn wire_id(self) -> u8 {
        match self {
            Target::Keyboard => 1,
            Target::MouseRel => 2,
            Target::MouseAbs => 3,
            Target::Pad(slot) => 0x10 + slot,
        }
    }

    pub const fn payload_len(self) -> usize {
        match self {
            Target::Keyboard | Target::Pad(_) => 16,
            Target::MouseRel => 7,
            Target::MouseAbs => 5,
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

/// What the driver feeds when the feeder closes (release-all): no keys, no buttons, every pad slot
/// centred. Absolute mouse is left out so the cursor does not jump.
pub fn neutral_reports() -> [Report; 2 + PAD_SLOTS as usize] {
    let mut all = [Report::new(Target::Keyboard, [0; PAYLOAD_MAX]); 2 + PAD_SLOTS as usize];
    all[1] = Report::new(Target::MouseRel, [0; PAYLOAD_MAX]);
    let mut slot = 0;
    while slot < PAD_SLOTS {
        all[2 + slot as usize] = PadState::NEUTRAL.report(slot);
        slot += 1;
    }
    all
}
