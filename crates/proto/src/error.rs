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
