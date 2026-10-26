//! Byte layout of the `vhid` control IOCTLs (spec 0016), mirrored in `drivers/vhid/Control.h`.
//! Fields are little-endian and packed; every request starts with a u32 `version`.

/// `CTL_CODE(FILE_DEVICE_UNKNOWN, 0x900, METHOD_BUFFERED, FILE_ANY_ACCESS)`.
pub const IOCTL_FEED: u32 = 0x0022_2400;
pub const IOCTL_QUERY: u32 = 0x0022_2404;

pub const WIRE_VERSION: u32 = 1;
pub const QUERY_IN_SIZE: usize = 4;
pub const QUERY_SIZE: usize = 16;

/// `QUERY` flags bit 0: a feeder owns the device.
const FLAG_FEEDER_HELD: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueryReply {
    pub version: u32,
    /// Reports dropped because the ring was full.
    pub dropped: u32,
    pub feeder_held: bool,
    /// Payload sizes the driver uses: keyboard, mouse relative, mouse absolute, gamepad.
    pub sizes: [u8; 4],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum WireError {
    #[error("the driver's reply is too short")]
    Short,
    #[error("the driver speaks another wire version")]
    Version,
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
    Ok(QueryReply {
        version,
        dropped: u32_at(4),
        feeder_held: u32_at(8) & FLAG_FEEDER_HELD != 0,
        sizes: [bytes[12], bytes[13], bytes[14], bytes[15]],
    })
}

#[cfg(test)]
#[path = "vhid_wire_tests.rs"]
mod tests;
