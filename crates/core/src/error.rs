//! Typed errors for core modules.

/// Bad text form of an id (`DeviceId`).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum IdParseError {
    #[error("device id must be 32 hex digits, got {len} bytes")]
    Length { len: usize },
    #[error("device id has a non-hex character at byte {index}")]
    NotHex { index: usize },
}

/// Bad hex text for a key or byte string.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HexError {
    #[error("expected {expected} hex digits, got {len}")]
    Length { expected: usize, len: usize },
    #[error("odd number of hex digits ({len})")]
    OddLength { len: usize },
    #[error("{len} bytes, at most {max} allowed")]
    TooLong { max: usize, len: usize },
    #[error("non-hex character at byte {index}")]
    NotHex { index: usize },
}

/// Config text that cannot be used.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("failed to parse config: {0}")]
    Parse(#[source] serde_json::Error),
    #[error("failed to encode config: {0}")]
    Encode(#[source] serde_json::Error),
    #[error("config version {found} is not supported (expected {expected})")]
    UnsupportedVersion { found: u32, expected: u32 },
    #[error("config `{field}` must not be 0")]
    ZeroPort { field: &'static str },
    #[error("discovery and session ports are both {port}")]
    PortClash { port: u16 },
    #[error("max_fps {value} is outside 1-240")]
    FpsOutOfRange { value: u32 },
    #[error("bitrate_kbps {value} is outside 1000-500000")]
    BitrateOutOfRange { value: u32 },
    #[error("name is empty")]
    EmptyName,
    #[error("name is {len} bytes, at most 63 allowed")]
    NameTooLong { len: usize },
}
