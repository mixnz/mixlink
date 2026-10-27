//! Control-channel messages (JSON): handshake with the client's displays, reject, bye,
//! pairing and the session handshake (spec 0018).
//! Not a hot path. Unknown fields are ignored, so later tasks add fields without a version bump.
//! The handshake is idempotent: the client resends `Hello` until `Welcome` or `Reject` arrives.

use std::ops::RangeInclusive;

use mixlink_core::config::MAX_NAME_BYTES;
use mixlink_core::ids::{DeviceId, SessionId};
use serde::{Deserialize, Serialize};

pub use crate::blob::{Blob, PublicKey};
use crate::error::ControlError;

const MAX_DISPLAYS: usize = 8;
const DISPLAY_SIZE: RangeInclusive<u32> = 1..=16_384;
/// 1-1000 Hz in millihertz.
const REFRESH_MHZ: RangeInclusive<u32> = 1_000..=1_000_000;
/// SPAKE2 (Ed25519 group) message: side byte + 32-byte element.
pub const SPAKE_LEN: usize = 33;
/// HMAC-SHA256.
pub const MAC_LEN: usize = 32;
/// Noise handshake message with a `Hello` / `Welcome` payload.
pub const MAX_NOISE: usize = 2048;

/// One client display; the host builds its virtual monitor from it (ADR-0006).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DisplayInfo {
    pub width: u32,
    pub height: u32,
    /// Millihertz, so 59.94 Hz is exactly 59_940.
    pub refresh_mhz: u32,
    pub hdr: bool,
    pub dpi: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ControlMessage {
    /// Client -> host. `stream_display` indexes `displays`: the display the stream window is on.
    Hello {
        proto: u8,
        device: DeviceId,
        name: String,
        displays: Vec<DisplayInfo>,
        stream_display: u8,
    },
    /// Host -> client: the mode actually streamed.
    Welcome {
        session: SessionId,
        mode: DisplayInfo,
    },
    Reject {
        reason: RejectReason,
    },
    Bye {
        reason: ByeReason,
    },
    /// Client -> host, plaintext (spec 0018): asks the host to show a PIN. `host` = the id the
    /// client found by discovery; `key` = the client's static public key.
    PairRequest {
        proto: u8,
        device: DeviceId,
        host: DeviceId,
        name: String,
        key: PublicKey,
    },
    /// Host -> client: the PIN is on the host's screen for `expires_s` seconds.
    PairPrompt {
        expires_s: u32,
    },
    /// Client -> host: SPAKE2 message A (from the PIN the user typed).
    PairStart {
        device: DeviceId,
        spake: Blob,
    },
    /// Host -> client: SPAKE2 message B, host static key, host confirm MAC.
    PairReply {
        spake: Blob,
        key: PublicKey,
        mac: Blob,
    },
    /// Client -> host: client confirm MAC.
    PairConfirm {
        device: DeviceId,
        mac: Blob,
    },
    /// Host -> client: the client is stored as paired.
    PairDone,
    /// Client -> host, plaintext: Noise `KK` message 1; its payload is the `Hello` JSON.
    SessionInit {
        proto: u8,
        device: DeviceId,
        host: DeviceId,
        noise: Blob,
    },
    /// Host -> client, plaintext: Noise message 2; its payload is `Welcome` or `Reject` JSON.
    SessionAccept {
        noise: Blob,
    },
    /// Sealed, both ways once the session is up; the client sends one every second.
    Ping {
        n: u32,
    },
    Pong {
        n: u32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RejectReason {
    Version,
    Busy,
    NotPaired,
    BadDisplay,
    /// Pairing: the PIN confirm failed.
    BadPin,
    /// Pairing: no prompt open for this client (expired, cancelled, too many tries, wrong host id).
    PairingClosed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ByeReason {
    UserClosed,
    Shutdown,
    Timeout,
}

impl ControlMessage {
    pub fn validate(&self) -> Result<(), ControlError> {
        match self {
            ControlMessage::Hello {
                name,
                displays,
                stream_display,
                ..
            } => {
                validate_name(name)?;
                if displays.is_empty() {
                    return Err(ControlError::NoDisplays);
                }
                if displays.len() > MAX_DISPLAYS {
                    return Err(ControlError::TooManyDisplays {
                        count: displays.len(),
                    });
                }
                if usize::from(*stream_display) >= displays.len() {
                    return Err(ControlError::StreamDisplay {
                        index: *stream_display,
                        count: displays.len(),
                    });
                }
                displays.iter().try_for_each(validate_display)
            }
            ControlMessage::Welcome { mode, .. } => validate_display(mode),
            ControlMessage::PairRequest { name, .. } => validate_name(name),
            ControlMessage::PairStart { spake, .. } => {
                blob_len("spake", spake, SPAKE_LEN..=SPAKE_LEN)
            }
            ControlMessage::PairReply { spake, mac, .. } => {
                blob_len("spake", spake, SPAKE_LEN..=SPAKE_LEN)?;
                blob_len("mac", mac, MAC_LEN..=MAC_LEN)
            }
            ControlMessage::PairConfirm { mac, .. } => blob_len("mac", mac, MAC_LEN..=MAC_LEN),
            ControlMessage::SessionInit { noise, .. } | ControlMessage::SessionAccept { noise } => {
                blob_len("noise", noise, 1..=MAX_NOISE)
            }
            ControlMessage::Reject { .. }
            | ControlMessage::Bye { .. }
            | ControlMessage::PairPrompt { .. }
            | ControlMessage::PairDone
            | ControlMessage::Ping { .. }
            | ControlMessage::Pong { .. } => Ok(()),
        }
    }
}

fn blob_len(
    field: &'static str,
    blob: &Blob,
    allowed: RangeInclusive<usize>,
) -> Result<(), ControlError> {
    if allowed.contains(&blob.0.len()) {
        Ok(())
    } else {
        Err(ControlError::BlobLen {
            field,
            len: blob.0.len(),
        })
    }
}

/// Validates, then encodes as JSON.
pub fn encode(message: &ControlMessage) -> Result<Vec<u8>, ControlError> {
    message.validate()?;
    serde_json::to_vec(message).map_err(ControlError::Encode)
}

/// Parses JSON, then validates.
pub fn decode(bytes: &[u8]) -> Result<ControlMessage, ControlError> {
    let message: ControlMessage = serde_json::from_slice(bytes).map_err(ControlError::Parse)?;
    message.validate()?;
    Ok(message)
}

fn validate_display(display: &DisplayInfo) -> Result<(), ControlError> {
    if !DISPLAY_SIZE.contains(&display.width) || !DISPLAY_SIZE.contains(&display.height) {
        return Err(ControlError::DisplaySize {
            width: display.width,
            height: display.height,
        });
    }
    if !REFRESH_MHZ.contains(&display.refresh_mhz) {
        return Err(ControlError::Refresh {
            mhz: display.refresh_mhz,
        });
    }
    Ok(())
}

/// Same rule as `core::config` names: not blank, at most 63 UTF-8 bytes.
fn validate_name(name: &str) -> Result<(), ControlError> {
    if name.trim().is_empty() {
        return Err(ControlError::EmptyName);
    }
    if name.len() > MAX_NAME_BYTES {
        return Err(ControlError::NameTooLong { len: name.len() });
    }
    Ok(())
}

#[cfg(test)]
#[path = "control_tests.rs"]
mod tests;
