//! Launcher <-> stream process control messages: one JSON object per line over the child's
//! stdin/stdout. Control plane only; never carries media or input.

use serde::Deserialize;
use serde::Serialize;
use serde::de::DeserializeOwned;

/// Bumped on any breaking change to the messages below.
pub const IPC_VERSION: u32 = 1;

/// Sent by `mixlink-stream` on its stdout.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum StreamToLauncher {
    /// First frame presented; sent once.
    Ready { ipc_version: u32 },
    /// Window is closing; the process exits right after.
    Closed { reason: CloseReason },
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
    Error { message: String },
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
