//! Discovery errors. Bad probes and replies from the network are dropped, never errors.

use std::io;

#[derive(Debug, thiserror::Error)]
pub enum DiscoveryError {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("{call} failed with code {code:#x}")]
    Win32 { call: &'static str, code: u32 },
    #[error("cannot encode the discovery reply: {0}")]
    Json(#[from] serde_json::Error),
    #[error("discovery reply is {len} bytes, more than the limit")]
    ReplyTooLong { len: usize },
    #[error("discovery is not supported on this platform")]
    Unsupported,
    #[error("neither mDNS nor the broadcast responder could start")]
    NothingStarted,
}
