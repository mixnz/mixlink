//! EDID 1.4 for a virtual monitor with a stable identity per client (spec 0015).
//! Windows keys its per-monitor settings on manufacturer + product + serial, so the same client
//! always gets the same monitor. Modes come from the IOCTL mode list, not from the EDID.

use mixlink_core::ids::DeviceId;
use mixlink_proto::control::DisplayInfo;

use super::cvt::{Timing, cvt_rb};

const BLOCK: usize = 128;
/// `MXL` as three 5-bit letters, big-endian.
pub const MXL_MFG: [u8; 2] = [0x37, 0x0C];
pub const PRODUCT_CLIENT: u16 = 0x4D58;
pub const PRODUCT_HEADLESS: u16 = 0x4D59;
/// The monitor added when no physical monitor is active.
pub const HEADLESS_ID: DeviceId =
    DeviceId::from_bytes([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]);
pub const HEADLESS_MODE: DisplayInfo = DisplayInfo {
    width: 1920,
    height: 1080,
    refresh_mhz: 60_000,
    hdr: false,
    dpi: 96,
};

/// Year of manufacture, stored as year − 1990.
const YEAR: u8 = 36;
/// Digital input, 8 bits per colour, interface not defined.
const VIDEO_INPUT: u8 = 0xA0;
/// Gamma 2.2, stored as gamma × 100 − 100.
const GAMMA: u8 = 120;
/// sRGB default colour space, preferred timing is the native mode.
const FEATURES: u8 = 0x06;
const SRGB_CHROMA: [u8; 10] = [0xEE, 0x91, 0xA3, 0x54, 0x4C, 0x99, 0x26, 0x0F, 0x50, 0x54];
/// Largest pixel clock a detailed timing descriptor holds, kHz.
const DTD_MAX_KHZ: u32 = 655_350;
/// Digital separate sync, hsync positive, vsync negative (CVT-RB).
const DTD_FLAGS: u8 = 0x1A;
const NAME: &[u8] = b"MixLink";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edid {
    pub base: [u8; BLOCK],
    /// CTA-861 and other extension blocks; none yet (HDR later, spec 0015).
    pub extensions: Vec<[u8; BLOCK]>,
}

impl Edid {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = self.base.to_vec();
        for block in &self.extensions {
            out.extend_from_slice(block);
        }
        out
    }
}

/// Builds the EDID for client `id` (or the headless monitor) at `mode`.
pub fn build(id: &DeviceId, product: u16, mode: &DisplayInfo) -> Edid {
    let bytes = id.as_bytes();
    let mut base = [0u8; BLOCK];
    base[0..8].copy_from_slice(&[0, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0]);
    base[8..10].copy_from_slice(&MXL_MFG);
    base[10..12].copy_from_slice(&product.to_le_bytes());
    base[12..16].copy_from_slice(&bytes[0..4]);
    base[17] = YEAR;
    base[18] = 1;
    base[19] = 4;
    base[20] = VIDEO_INPUT;
    let (width_cm, height_cm) = size_cm(mode);
    base[21] = width_cm;
    base[22] = height_cm;
    base[23] = GAMMA;
    base[24] = FEATURES;
    base[25..35].copy_from_slice(&SRGB_CHROMA);
    for slot in base[38..54].as_chunks_mut::<2>().0 {
        slot.copy_from_slice(&[1, 1]);
    }
    base[54..72].copy_from_slice(&dtd(&preferred_timing(mode), width_cm, height_cm));
    base[72..90].copy_from_slice(&text_descriptor(0xFF, &serial_text(bytes)));
    base[90..108].copy_from_slice(&text_descriptor(0xFC, NAME));
    base[108..126].copy_from_slice(&text_descriptor(0x10, &[]));
    base[127] = checksum(&base);
    Edid {
        base,
        extensions: Vec::new(),
    }
}

/// Monitor container id: the device id bytes as a version 4 GUID.
pub fn container_id(id: &DeviceId) -> [u8; 16] {
    let mut guid = *id.as_bytes();
    guid[6] = (guid[6] & 0x0F) | 0x40;
    guid[8] = (guid[8] & 0x3F) | 0x80;
    guid
}

/// Physical size from the client's dpi, so Windows' default scale is close to the client's.
fn size_cm(mode: &DisplayInfo) -> (u8, u8) {
    if mode.dpi == 0 {
        return (0, 0);
    }
    let cm = |px: u32| {
        let value = (f64::from(px) / f64::from(mode.dpi) * 2.54).round();
        value.clamp(1.0, 255.0) as u8
    };
    (cm(mode.width), cm(mode.height))
}

fn preferred_timing(mode: &DisplayInfo) -> Timing {
    let timing = cvt_rb(mode.width, mode.height, mode.refresh_mhz);
    if timing.pixel_khz <= DTD_MAX_KHZ {
        timing
    } else {
        cvt_rb(
            HEADLESS_MODE.width,
            HEADLESS_MODE.height,
            HEADLESS_MODE.refresh_mhz,
        )
    }
}

fn low(value: u32) -> u8 {
    (value & 0xFF) as u8
}

fn high(value: u32, bits: u32) -> u8 {
    ((value >> 8) & ((1 << bits) - 1)) as u8
}

fn dtd(t: &Timing, width_cm: u8, height_cm: u8) -> [u8; 18] {
    let clock = (t.pixel_khz / 10).to_le_bytes();
    let (h_mm, v_mm) = (u32::from(width_cm) * 10, u32::from(height_cm) * 10);
    [
        clock[0],
        clock[1],
        low(t.h_active),
        low(t.h_blank),
        (high(t.h_active, 4) << 4) | high(t.h_blank, 4),
        low(t.v_active),
        low(t.v_blank),
        (high(t.v_active, 4) << 4) | high(t.v_blank, 4),
        low(t.h_front),
        low(t.h_sync),
        ((low(t.v_front) & 0x0F) << 4) | (low(t.v_sync) & 0x0F),
        (high(t.h_front, 2) << 6)
            | (high(t.h_sync, 2) << 4)
            | (((t.v_front >> 4) & 0x03) as u8) << 2
            | ((t.v_sync >> 4) & 0x03) as u8,
        low(h_mm),
        low(v_mm),
        (high(h_mm, 4) << 4) | high(v_mm, 4),
        0,
        0,
        DTD_FLAGS,
    ]
}

/// Hex of the first 6 id bytes and a line feed: 13 bytes, the descriptor's full text.
fn serial_text(bytes: &[u8; 16]) -> Vec<u8> {
    let mut text: Vec<u8> = bytes[0..6]
        .iter()
        .flat_map(|b| format!("{b:02x}").into_bytes())
        .collect();
    text.push(b'\n');
    text
}

/// Display descriptor `tag` with up to 13 bytes of text, ended by a line feed and padded with spaces.
fn text_descriptor(tag: u8, text: &[u8]) -> [u8; 18] {
    let mut out = [0u8; 18];
    out[3] = tag;
    if tag == 0x10 {
        return out;
    }
    let field = &mut out[5..18];
    field.fill(b' ');
    let len = text.len().min(13);
    field[..len].copy_from_slice(&text[..len]);
    if len < 13 && text.last() != Some(&b'\n') {
        field[len] = b'\n';
    }
    out
}

fn checksum(block: &[u8; BLOCK]) -> u8 {
    let sum = block[..BLOCK - 1]
        .iter()
        .fold(0u8, |acc, &b| acc.wrapping_add(b));
    0u8.wrapping_sub(sum)
}

#[cfg(test)]
#[path = "edid_tests.rs"]
mod tests;
