//! Launcher <-> stream process control messages: one JSON object per line over the child's
//! stdin/stdout. Control plane only; never carries media or input.

use serde::Deserialize;
use serde::Serialize;
use serde::de::DeserializeOwned;

/// Bumped on any breaking change to the messages below.
pub const IPC_VERSION: u32 = 2;

/// Sent by `mixlink-stream` on its stdout.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum StreamToLauncher {
    /// First frame presented; sent once.
    Ready { ipc_version: u32 },
    /// Connect mode (spec 0020): the secure session handshake started.
    Connecting,
    /// Connect mode: the host accepted; the streamed mode and codec (`h264` / `hevc`).
    Connected {
        width: u32,
        height: u32,
        refresh_mhz: u32,
        codec: String,
    },
    /// Window is closing; the process exits right after. `latency`: the session's per-stage summary.
    Closed {
        reason: CloseReason,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        latency: Option<LatencySummary>,
    },
}

/// Per-stage latency of one session (spec 0020). Span names: `capture`, `encode`, `send`, `network`,
/// `reassemble`, `queue`, `decode`, `present`, `total`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LatencySummary {
    pub frames: u64,
    pub spans: Vec<SpanSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpanSummary {
    pub name: String,
    pub p50_us: u32,
    pub p99_us: u32,
}

/// Sent by the launcher on the stream's stdin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum LauncherToStream {
    Stop,
}

/// Why the stream window closed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum CloseReason {
    User,
    Stopped,
    Error {
        message: String,
    },
    /// The host refused the session: `busy`, `badDisplay` or `version`.
    Rejected {
        reason: String,
    },
    /// The host closed the session: `userClosed`, `shutdown`, `hostError` or `timeout`.
    HostEnded {
        reason: String,
    },
    /// No valid datagram from the host for 10 s.
    ConnectionLost,
    /// The handshake got no answer.
    HostUnreachable,
    /// The host no longer knows this client; it was forgotten here.
    NotPaired,
}

#[derive(Debug, thiserror::Error)]
pub enum IpcError {
    #[error("failed to encode IPC message: {0}")]
    Encode(#[source] serde_json::Error),
    #[error("failed to decode IPC message: {0}")]
    Decode(#[source] serde_json::Error),
}

/// Serializes `msg` as one line, including the trailing `\n`.
pub fn encode_line<T: Serialize>(msg: &T) -> Result<String, IpcError> {
    let mut line = serde_json::to_string(msg).map_err(IpcError::Encode)?;
    line.push('\n');
    Ok(line)
}

/// Parses one line; a trailing `\n` or `\r\n` is ignored.
pub fn decode_line<T: DeserializeOwned>(line: &str) -> Result<T, IpcError> {
    serde_json::from_str(line.trim_end_matches(['\r', '\n'])).map_err(IpcError::Decode)
}

#[cfg(test)]
#[path = "ipc_tests.rs"]
mod tests;
