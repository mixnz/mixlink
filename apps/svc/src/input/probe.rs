//! `input probe` (spec 0016), pure part: sequence codes carried in side-effect-free input, matching
//! arrivals to send times after the run, nearest-rank summary. Ported from the spec 0006 harness.

use mixlink_input::hid::keyboard::KeyboardState;
use mixlink_input::hid::mouse::{mouse_abs_report, mouse_rel_reports};
use mixlink_input::hid::report::Report;
use mixlink_input::hid::xbox::PadState;

pub use crate::args::ProbeDevice;

/// HID usage of F13 (F13..F24 = 0x68..0x73).
pub const USAGE_F13: u8 = 0x68;
/// Windows virtual-key code of F13 (F13..F24 = 0x7C..0x87).
pub const VK_F13: u16 = 0x7C;
/// 12 keys × (press, release).
pub const KEY_PERIOD: u32 = 24;
/// +d / −d pairs with d = 1..=100, so the cursor comes back.
pub const REL_PERIOD: u32 = 200;
pub const ABS_PERIOD: u32 = 1000;
/// Below 0x8000: a centred stick (0x8000) is never a code.
pub const PAD_PERIOD: u32 = 32_768;

const ABS_BASE: i32 = 1000;
const ABS_STEP: i32 = 30;
const ABS_TOLERANCE: i32 = 2;
/// Absolute Y for the abs run: mid-screen.
const ABS_Y: u16 = 16_384;

pub const fn period(device: ProbeDevice) -> u32 {
    match device {
        ProbeDevice::Kbd => KEY_PERIOD,
        ProbeDevice::Rel => REL_PERIOD,
        ProbeDevice::Abs => ABS_PERIOD,
        ProbeDevice::Pad => PAD_PERIOD,
    }
}

/// The report for event `i`. `keyboard` keeps key state across events.
// HOT PATH
pub fn report_for(device: ProbeDevice, i: u32, keyboard: &mut KeyboardState, slot: u8) -> Report {
    match device {
        ProbeDevice::Kbd => {
            let (usage, down) = encode_key(i);
            keyboard.set(usage, down);
            keyboard.report()
        }
        ProbeDevice::Rel => mouse_rel_reports(0, i32::from(encode_rel(i)), 0, 0, 0).as_slice()[0],
        ProbeDevice::Abs => mouse_abs_report(0, encode_abs(i), ABS_Y),
        ProbeDevice::Pad => PadState {
            lx: encode_pad_lx(i),
            ..PadState::NEUTRAL
        }
        .report(slot),
    }
}

/// Event `i`: F13 + (i/2 % 12), pressed on even `i`, released on odd.
pub fn encode_key(i: u32) -> (u8, bool) {
    let k = u8::try_from((i / 2) % 12).unwrap_or(0);
    (USAGE_F13 + k, i.is_multiple_of(2))
}

pub fn decode_key(vkey: u16, is_break: bool) -> Option<u32> {
    let k = vkey.checked_sub(VK_F13)?;
    (k < 12).then(|| u32::from(k) * 2 + u32::from(is_break))
}

/// Event `i`: magnitude (i/2 % 100) + 1, positive on even `i`, negative on odd.
pub fn encode_rel(i: u32) -> i16 {
    let m = i16::try_from((i / 2) % 100).unwrap_or(0) + 1;
    if i.is_multiple_of(2) { m } else { -m }
}

pub fn decode_rel(dx: i32) -> Option<u32> {
    let m = dx.unsigned_abs();
    match dx {
        1..=100 => Some((m - 1) * 2),
        -100..=-1 => Some((m - 1) * 2 + 1),
        _ => None,
    }
}

/// Event `i`: X = 1000 + (i % 1000) × 30 in the descriptor's 0..=32767 range.
pub fn encode_abs(i: u32) -> u16 {
    let r = i32::try_from(i % ABS_PERIOD).unwrap_or(0);
    u16::try_from(ABS_BASE + r * ABS_STEP).unwrap_or(0)
}

/// `normalized` is Raw Input's 0..=65535 absolute X.
pub fn decode_abs(normalized: i32) -> Option<u32> {
    let x = (f64::from(normalized) * 32_767.0 / 65_535.0).round();
    let r = ((x - f64::from(ABS_BASE)) / f64::from(ABS_STEP)).round();
    if !(0.0..f64::from(ABS_PERIOD)).contains(&r) {
        return None;
    }
    let expected = f64::from(ABS_BASE) + r * f64::from(ABS_STEP);
    if (expected - x).abs() > f64::from(ABS_TOLERANCE) {
        return None;
    }
    // `r` is a whole number in 0..1000.
    Some(r as u32)
}

/// Stick value whose report bytes read `i % PAD_PERIOD` (report = value XOR 0x8000).
pub fn encode_pad_lx(i: u32) -> i16 {
    let code = u16::try_from(i % PAD_PERIOD).unwrap_or(0);
    (code ^ 0x8000).cast_signed()
}

/// `raw` is the report's LX field (bytes 0..2 of the payload).
pub fn decode_pad_lx(raw: u16) -> Option<u32> {
    let v = u32::from(raw);
    (v < PAD_PERIOD).then_some(v)
}

/// XInput `sThumbLX` of a pad fed with code `i % PAD_PERIOD`: xinputhid maps the HID axis
/// 0..=65535 onto -32768..=32767, so the report value is the thumb XOR 0x8000.
pub fn decode_xinput_lx(thumb: i16) -> Option<u32> {
    decode_pad_lx(thumb.cast_unsigned() ^ 0x8000)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Summary {
    pub p50_us: f64,
    pub p99_us: f64,
    pub max_us: f64,
    pub sent: usize,
    pub received: usize,
    pub lost: usize,
}

/// Nearest-rank percentile of an ascending slice.
pub fn nearest_rank(sorted: &[i64], pct: usize) -> Option<i64> {
    if sorted.is_empty() {
        return None;
    }
    let rank = (pct * sorted.len()).div_ceil(100).max(1);
    sorted.get(rank - 1).copied()
}

/// Matches arrivals `(residue, qpc)` to send times after the run: each belongs to the latest event
/// sent at or before it with that residue (`i mod period`), and indices only move forward. Stale
/// readings, duplicates and late repeats are dropped; a lost event does not shift later ones.
pub fn assign_arrivals(sent: &[i64], period: u32, arrivals: &[(u32, i64)]) -> Vec<Option<i64>> {
    let mut recv = vec![None; sent.len()];
    let mut next_min = 0usize;
    let p = usize::try_from(period).unwrap_or(1).max(1);
    for &(residue, t) in arrivals {
        let r = usize::try_from(residue).unwrap_or(usize::MAX);
        // Events are sent in index order, so "sent at or before t" is a prefix.
        let Some(k) = sent.partition_point(|&s| s <= t).checked_sub(1) else {
            continue;
        };
        if r >= p {
            continue;
        }
        let back = (k % p + p - r) % p;
        let Some(i) = k.checked_sub(back) else {
            continue;
        };
        if i < next_min {
            continue;
        }
        recv[i] = Some(t);
        next_min = i + 1;
    }
    recv
}

pub fn summary(sent: &[i64], recv: &[Option<i64>], qpc_freq: i64) -> Summary {
    let mut lat: Vec<i64> = sent
        .iter()
        .zip(recv)
        .filter_map(|(s, r)| Some((*r)? - *s))
        .collect();
    lat.sort_unstable();
    #[expect(clippy::cast_precision_loss, reason = "tick counts far below 2^52")]
    let to_us = |ticks: Option<i64>| ticks.map_or(0.0, |t| t as f64 * 1e6 / qpc_freq as f64);
    Summary {
        p50_us: to_us(nearest_rank(&lat, 50)),
        p99_us: to_us(nearest_rank(&lat, 99)),
        max_us: to_us(lat.last().copied()),
        sent: sent.len(),
        received: lat.len(),
        lost: sent.len() - lat.len(),
    }
}

#[cfg(test)]
#[path = "probe_tests.rs"]
mod tests;
