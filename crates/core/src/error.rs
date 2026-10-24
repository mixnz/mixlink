//! Typed errors for core modules.

/// Bad text form of an id (`DeviceId`).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum IdParseError {
    #[error("device id must be 32 hex digits, got {len} bytes")]
    Length { len: usize },
    #[error("device id has a non-hex character at byte {index}")]
    NotHex { index: usize },
}
