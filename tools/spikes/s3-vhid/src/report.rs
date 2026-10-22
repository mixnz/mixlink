//! Report bytes for `drivers/vhid` (layout: `drivers/vhid/Descriptor.c`).

use std::fmt;

pub const CONTROL_REPORT_ID: u8 = 5;
/// `[ID 5][target ID][PAYLOAD_MAX bytes]`.
pub const CONTROL_REPORT_LEN: usize = 2 + PAYLOAD_MAX;
pub const PAYLOAD_MAX: usize = 13;
/// HID keyboard usage of F13 (F13..F24 = 0x68..0x73).
pub const USAGE_F13: u8 = 0x68;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Device {
    Keyboard,
    MouseRel,
    MouseAbs,
    Gamepad,
}

pub fn report_id(device: Device) -> u8 {
    match device {
        Device::Keyboard => 1,
        Device::MouseRel => 2,
        Device::MouseAbs => 3,
        Device::Gamepad => 4,
    }
}

pub fn payload_len(device: Device) -> usize {
    match device {
        Device::Keyboard => 8,
        Device::MouseRel => 7,
        Device::MouseAbs => 5,
        Device::Gamepad => 13,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReportError {
    WrongLength { device: Device, len: usize },
}

impl fmt::Display for ReportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReportError::WrongLength { device, len } => write!(
                f,
                "{device:?} payload is {len} bytes, expected {}",
                payload_len(*device)
            ),
        }
    }
}

impl std::error::Error for ReportError {}

// HOT PATH
pub fn control_report(
    device: Device,
    payload: &[u8],
) -> Result<[u8; CONTROL_REPORT_LEN], ReportError> {
    if payload.len() != payload_len(device) {
        return Err(ReportError::WrongLength {
            device,
            len: payload.len(),
        });
    }
    let mut report = [0u8; CONTROL_REPORT_LEN];
    report[0] = CONTROL_REPORT_ID;
    report[1] = report_id(device);
    report[2..2 + payload.len()].copy_from_slice(payload);
    Ok(report)
}

/// Gamepad state as the HID report carries it. Sticks 0..65535 (0x8000 = center),
/// hat 0..7 clockwise from up, 8 = centered (null).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PadState {
    pub buttons: u16,
    pub hat: u8,
    pub lx: u16,
    pub ly: u16,
    pub rx: u16,
    pub ry: u16,
    pub lt: u8,
    pub rt: u8,
}

impl PadState {
    pub const CENTERED: PadState = PadState {
        buttons: 0,
        hat: 8,
        lx: 0x8000,
        ly: 0x8000,
        rx: 0x8000,
        ry: 0x8000,
        lt: 0,
        rt: 0,
    };
}

pub fn pad_payload(state: &PadState) -> [u8; 13] {
    let mut p = [0u8; 13];
    p[0..2].copy_from_slice(&state.buttons.to_le_bytes());
    p[2] = state.hat & 0x0F;
    p[3..5].copy_from_slice(&state.lx.to_le_bytes());
    p[5..7].copy_from_slice(&state.ly.to_le_bytes());
    p[7..9].copy_from_slice(&state.rx.to_le_bytes());
    p[9..11].copy_from_slice(&state.ry.to_le_bytes());
    p[11] = state.lt;
    p[12] = state.rt;
    p
}

/// Boot-layout keyboard report; keys beyond six are dropped.
pub fn kbd_payload(modifiers: u8, keys: &[u8]) -> [u8; 8] {
    let mut p = [0u8; 8];
    p[0] = modifiers;
    for (slot, key) in p[2..].iter_mut().zip(keys) {
        *slot = *key;
    }
    p
}

pub fn mouse_rel_payload(buttons: u8, dx: i16, dy: i16, wheel: i8) -> [u8; 7] {
    let mut p = [0u8; 7];
    p[0] = buttons & 0x1F;
    p[1..3].copy_from_slice(&dx.to_le_bytes());
    p[3..5].copy_from_slice(&dy.to_le_bytes());
    p[5] = wheel.to_le_bytes()[0];
    p
}

pub fn mouse_abs_payload(buttons: u8, x: u16, y: u16) -> [u8; 5] {
    let mut p = [0u8; 5];
    p[0] = buttons & 0x1F;
    p[1..3].copy_from_slice(&x.to_le_bytes());
    p[3..5].copy_from_slice(&y.to_le_bytes());
    p
}

#[cfg(test)]
#[path = "report_tests.rs"]
mod tests;
