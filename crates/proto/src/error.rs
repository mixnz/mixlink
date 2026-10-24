//! Typed errors for proto modules.

/// A datagram that is not a valid v1 packet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum DecodeError {
    #[error("datagram is {len} bytes, shorter than the 16-byte header")]
    TooShort { len: usize },
    #[error("wire version {found} is not supported")]
    Version { found: u8 },
    #[error("unknown channel id {id}")]
    Channel { id: u8 },
    #[error("unknown flag bits {bits:#06x}")]
    Flags { bits: u16 },
    #[error("fragment count is 0")]
    FragCount,
    #[error("fragment index {index} is not below fragment count {count}")]
    FragIndex { index: u16, count: u16 },
    #[error("payload is {len} bytes, at most 1200 allowed")]
    PayloadTooLarge { len: usize },
    #[error("fragment {index} is {len} bytes; every fragment but the last must be 1200")]
    ShortFragment { index: u16, len: usize },
}

/// A message the packetizer cannot split.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PacketizeError {
    #[error("message is {len} bytes, more than 65535 fragments")]
    TooLarge { len: usize },
}

/// A control message that cannot be parsed or breaks a handshake rule.
#[derive(Debug, thiserror::Error)]
pub enum ControlError {
    #[error("failed to parse control message: {0}")]
    Parse(#[source] serde_json::Error),
    #[error("failed to encode control message: {0}")]
    Encode(#[source] serde_json::Error),
    #[error("hello lists no displays")]
    NoDisplays,
    #[error("hello lists {count} displays, at most 8 allowed")]
    TooManyDisplays { count: usize },
    #[error("stream display {index} is out of range for {count} displays")]
    StreamDisplay { index: u8, count: usize },
    #[error("display size {width}x{height} is outside 1-16384")]
    DisplaySize { width: u32, height: u32 },
    #[error("refresh {mhz} mHz is outside 1-1000 Hz")]
    Refresh { mhz: u32 },
    #[error("name is empty")]
    EmptyName,
    #[error("name is {len} bytes, at most 63 allowed")]
    NameTooLong { len: usize },
}

/// A cursor message that does not follow the fixed layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum CursorError {
    #[error("cursor message is {len} bytes, too short")]
    TooShort { len: usize },
    #[error("unknown cursor message kind {byte}")]
    Kind { byte: u8 },
    #[error("unknown cursor shape kind {byte}")]
    ShapeKind { byte: u8 },
    #[error("cursor visible byte is {byte}, expected 0 or 1")]
    Visible { byte: u8 },
    #[error("cursor shape has {found} pixel bytes, pitch x height is {expected}")]
    PixelLen { expected: usize, found: usize },
    #[error("monochrome cursor height {height} is odd (AND + XOR masks)")]
    MonochromeHeight { height: u16 },
    #[error("output buffer is {have} bytes, cursor shape needs {need}")]
    BufferTooSmall { need: usize, have: usize },
}
