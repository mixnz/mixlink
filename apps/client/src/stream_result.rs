//! How a stream session ended, as a typed value for the UI (ADR-0003: no sentences in Rust).

use mixlink_core::ipc::{CloseReason, IPC_VERSION};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum StreamResult {
    ClosedByUser,
    Stopped,
    StreamError {
        message: String,
    },
    Crashed {
        code: Option<i32>,
    },
    VersionMismatch {
        stream: u32,
        launcher: u32,
    },
    SpawnFailed {
        message: String,
    },
    /// The host refused: `busy`, `badDisplay`, `version` (spec 0020).
    Rejected {
        reason: String,
    },
    /// The host closed the session: `userClosed`, `shutdown`, `hostError`, `timeout`.
    HostEnded {
        reason: String,
    },
    ConnectionLost,
    HostUnreachable,
    NotPaired,
}

/// `ready`: version from the stream's `Ready`, if any; `closed`: reason from its `Closed`, if any.
pub fn classify(
    ready: Option<u32>,
    closed: Option<CloseReason>,
    exit_code: Option<i32>,
) -> StreamResult {
    if let Some(stream) = ready.filter(|&version| version != IPC_VERSION) {
        return StreamResult::VersionMismatch {
            stream,
            launcher: IPC_VERSION,
        };
    }
    match closed {
        Some(CloseReason::User) => StreamResult::ClosedByUser,
        Some(CloseReason::Stopped) => StreamResult::Stopped,
        Some(CloseReason::Error { message }) => StreamResult::StreamError { message },
        Some(CloseReason::Rejected { reason }) => StreamResult::Rejected { reason },
        Some(CloseReason::HostEnded { reason }) => StreamResult::HostEnded { reason },
        Some(CloseReason::ConnectionLost) => StreamResult::ConnectionLost,
        Some(CloseReason::HostUnreachable) => StreamResult::HostUnreachable,
        Some(CloseReason::NotPaired) => StreamResult::NotPaired,
        None => StreamResult::Crashed { code: exit_code },
    }
}

#[cfg(test)]
#[path = "stream_result_tests.rs"]
mod tests;
