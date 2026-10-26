//! Byte layout of the `vhid` control IOCTLs (spec 0016), mirrored in `drivers/vhid/Control.h`.
//! Fields are little-endian and packed; every request starts with a u32 `version`.

use crate::hid::report::{Report, Target};

/// `CTL_CODE(FILE_DEVICE_UNKNOWN, 0x900, METHOD_BUFFERED, FILE_ANY_ACCESS)`.
pub const IOCTL_FEED: u32 = 0x0022_2400;
pub const IOCTL_QUERY: u32 = 0x0022_2404;

/// 2: pad targets and the pads-present byte.
pub const WIRE_VERSION: u32 = 2;
pub const FEED_SIZE: usize = 24;
pub const QUERY_IN_SIZE: usize = 4;
pub const QUERY_SIZE: usize = 20;
const FEED_PAYLOAD_AT: usize = 8;
const QUERY_SIZES_AT: usize = 12;
const QUERY_PADS_AT: usize = 16;

/// Payload sizes this build expects, in `QUERY` order: keyboard, mouse rel, mouse abs, pad.
const SIZES: [usize; 4] = [
    Target::Keyboard.payload_len(),
    Target::MouseRel.payload_len(),
    Target::MouseAbs.payload_len(),
    Target::Pad(0).payload_len(),
];

/// `QUERY` flags bit 0: a feeder owns the devices.
const FLAG_FEEDER_HELD: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueryReply {
    pub version: u32,
    /// Reports dropped because a ring was full (all nodes).
    pub dropped: u32,
    pub feeder_held: bool,
    /// Payload sizes the driver uses: keyboard, mouse relative, mouse absolute, pad.
    pub sizes: [u8; 4],
    /// Bit N: pad slot N has a node.
    pub pads_present: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum WireError {
    #[error("the driver's reply is too short")]
    Short,
    #[error("the driver speaks another wire version")]
    Version,
    #[error("the driver's report layouts differ from this program's")]
    Sizes,
}

/// `FEED`: version, target, payload length, reserved 0, payload (zero-padded to 16 B).
// HOT PATH
pub fn encode_feed(report: &Report) -> [u8; FEED_SIZE] {
    let payload = report.payload();
    let mut out = [0u8; FEED_SIZE];
    out[0..4].copy_from_slice(&WIRE_VERSION.to_le_bytes());
    out[4] = report.target().wire_id();
    out[5] = u8::try_from(payload.len()).unwrap_or(0);
    out[FEED_PAYLOAD_AT..FEED_PAYLOAD_AT + payload.len()].copy_from_slice(payload);
    out
}

pub fn encode_query() -> [u8; QUERY_IN_SIZE] {
    WIRE_VERSION.to_le_bytes()
}

pub fn decode_query(bytes: &[u8]) -> Result<QueryReply, WireError> {
    let bytes: &[u8; QUERY_SIZE] = bytes
        .get(..QUERY_SIZE)
        .and_then(|b| b.try_into().ok())
        .ok_or(WireError::Short)?;
    let u32_at =
        |at: usize| u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]);
    let version = u32_at(0);
    if version != WIRE_VERSION {
        return Err(WireError::Version);
    }
    let sizes = [
        bytes[QUERY_SIZES_AT],
        bytes[QUERY_SIZES_AT + 1],
        bytes[QUERY_SIZES_AT + 2],
        bytes[QUERY_SIZES_AT + 3],
    ];
    if sizes
        .iter()
        .zip(SIZES)
        .any(|(&got, want)| usize::from(got) != want)
    {
        return Err(WireError::Sizes);
    }
    Ok(QueryReply {
        version,
        dropped: u32_at(4),
        feeder_held: u32_at(8) & FLAG_FEEDER_HELD != 0,
        sizes,
        pads_present: bytes[QUERY_PADS_AT],
    })
}

#[cfg(test)]
#[path = "vhid_wire_tests.rs"]
mod tests;
