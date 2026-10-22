//! Sequence numbers carried in side-effect-free input (spec 0006 → Latency measurement).
//! Each device encodes `i mod PERIOD`; the receiver unwraps it back to `i`.

use crate::report::USAGE_F13;

/// Windows virtual-key code of F13 (F13..F24 = 0x7C..0x87).
pub const VK_F13: u16 = 0x7C;
/// 12 keys x (press, release).
pub const KEY_PERIOD: u32 = 24;
/// +d / -d pairs with d = 1..=100.
pub const REL_PERIOD: u32 = 200;
pub const ABS_PERIOD: u32 = 1000;
pub const PAD_PERIOD: u32 = 65536;

const ABS_BASE: i32 = 1000;
const ABS_STEP: i32 = 30;
const ABS_TOLERANCE: i32 = 2;

/// Event `i`: F13 + (i/2 % 12), pressed on even `i`, released on odd.
pub fn encode_key(i: u32) -> (u8, bool) {
    let k = ((i / 2) % 12) as u8;
    (USAGE_F13 + k, i.is_multiple_of(2))
}

pub fn decode_key(vkey: u16, is_break: bool) -> Option<u32> {
    let k = vkey.checked_sub(VK_F13)?;
    if k >= 12 {
        return None;
    }
    Some(u32::from(k) * 2 + u32::from(is_break))
}

/// Event `i`: magnitude (i/2 % 100) + 1, positive on even `i`, negative on odd.
pub fn encode_rel(i: u32) -> i16 {
    let m = ((i / 2) % 100) as i16 + 1;
    if i.is_multiple_of(2) { m } else { -m }
}

pub fn decode_rel(dx: i32) -> Option<u32> {
    match dx {
        1..=100 => Some((dx as u32 - 1) * 2),
        -100..=-1 => Some(((-dx) as u32 - 1) * 2 + 1),
        _ => None,
    }
}

/// Event `i`: X = 1000 + (i % 1000) * 30 in the descriptor's 0..32767 range.
pub fn encode_abs(i: u32) -> u16 {
    (ABS_BASE + (i % ABS_PERIOD) as i32 * ABS_STEP) as u16
}

/// `normalized` is Raw Input's 0..65535 absolute X.
pub fn decode_abs(normalized: i32) -> Option<u32> {
    let x = (f64::from(normalized) * 32767.0 / 65535.0).round() as i32;
    let r = (f64::from(x - ABS_BASE) / f64::from(ABS_STEP)).round() as i32;
    if !(0..ABS_PERIOD as i32).contains(&r) {
        return None;
    }
    if (ABS_BASE + r * ABS_STEP - x).abs() > ABS_TOLERANCE {
        return None;
    }
    Some(r as u32)
}

pub fn encode_pad_lx(i: u32) -> u16 {
    (i % PAD_PERIOD) as u16
}

pub fn decode_pad_lx(lx: u16) -> u32 {
    u32::from(lx)
}

/// XInput thumbs are signed: flip the top bit so 0x8000 (HID center) maps to 0.
pub fn encode_xinput_lx(i: u32) -> i16 {
    (encode_pad_lx(i) ^ 0x8000) as i16
}

pub fn decode_xinput_lx(v: i16) -> u32 {
    u32::from(v as u16 ^ 0x8000)
}

/// Windows.Gaming.Input axes are 0.0..=1.0.
pub fn decode_wgi_axis(v: f64) -> u32 {
    (v.clamp(0.0, 1.0) * 65535.0).round() as u32
}

/// Raw Input device handles a receiver accepts (our collections, or `0` for SendInput).
#[derive(Clone, Copy, Debug, Default)]
pub struct DeviceFilter {
    allowed: [isize; 4],
    len: usize,
}

impl DeviceFilter {
    pub fn new() -> Self {
        Self::default()
    }

    /// False when the filter is full.
    pub fn add(&mut self, handle: isize) -> bool {
        if self.accepts(handle) {
            return true;
        }
        if self.len == self.allowed.len() {
            return false;
        }
        self.allowed[self.len] = handle;
        self.len += 1;
        true
    }

    // HOT PATH
    pub fn accepts(&self, handle: isize) -> bool {
        self.allowed[..self.len].contains(&handle)
    }
}

/// Turns residues (`i mod period`) back into `i`. A residue that would jump more than
/// half a period ahead is a late or duplicate event: ignored, never moves back.
#[derive(Clone, Copy, Debug)]
pub struct Unwrapper {
    period: u32,
    last: Option<u32>,
}

impl Unwrapper {
    pub fn new(period: u32) -> Self {
        Self { period, last: None }
    }

    // HOT PATH
    pub fn next(&mut self, residue: u32) -> Option<u32> {
        if residue >= self.period {
            return None;
        }
        let candidate = match self.last {
            None => residue,
            Some(last) => {
                let mut c = last - last % self.period + residue;
                if c <= last {
                    c += self.period;
                }
                if c - last > self.period / 2 {
                    return None;
                }
                c
            }
        };
        self.last = Some(candidate);
        Some(candidate)
    }
}

#[cfg(test)]
#[path = "seq_tests.rs"]
mod tests;
