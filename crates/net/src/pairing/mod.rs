//! PIN pairing (spec 0018): SPAKE2 on the PIN, then both sides keep the peer's static key.

pub mod client;
pub mod host;
pub mod transcript;

use std::time::{Duration, Instant};

use mixlink_core::ids::DeviceId;
use mixlink_proto::control::PublicKey;
use serde::{Deserialize, Serialize};

use crate::error::NetError;

/// How long a PIN stays valid.
pub const PROMPT_TIME: Duration = Duration::from_secs(120);
/// `PairStart` attempts per prompt.
pub const MAX_ATTEMPTS: u8 = 3;

/// A client the host has paired with (`paired.json`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairedDevice {
    pub device: DeviceId,
    pub name: String,
    pub key: PublicKey,
    /// Unix seconds.
    pub paired_at: u64,
}

/// A host the client has paired with (`known_hosts.json`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnownHost {
    pub host: DeviceId,
    pub name: String,
    pub key: PublicKey,
}

/// Current time for the host state machines: monotonic for timers, wall clock for records.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Now {
    pub mono: Instant,
    pub unix_s: u64,
}

/// What the tray shows while a client is pairing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptView {
    pub device_name: String,
    /// Six digits, zero-padded.
    pub pin: String,
    pub expires_s: u32,
}

/// Why pairing did not finish.
#[derive(Debug, thiserror::Error)]
pub enum PairError {
    #[error("wrong PIN")]
    BadPin,
    #[error("the host closed the pairing prompt")]
    Closed,
    #[error("the host is pairing with another device")]
    Busy,
    #[error("the host runs another protocol version")]
    Version,
    #[error(transparent)]
    Net(#[from] NetError),
}

/// Six digits, zero-padded: the form the user sees and the SPAKE2 password.
pub fn pin_text(pin: u32) -> String {
    format!("{pin:06}")
}
