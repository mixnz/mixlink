//! What the host streams (spec 0020): the virtual monitor's mode and the codec, chosen from the client's
//! `Hello` during the session handshake, before the monitor exists. Control plane.

use mixlink_core::config::CodecChoice;
use mixlink_core::ids::SessionId;
use mixlink_proto::control::{CodecId, ControlMessage, DisplayInfo, RejectReason};

/// Largest mode the host streams.
const MAX_WIDTH: u32 = 3840;
const MAX_HEIGHT: u32 = 2160;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModePolicy {
    /// `video.max_fps`; a faster display is streamed at this rate.
    pub max_fps: u32,
    pub codec: CodecChoice,
}

impl ModePolicy {
    /// `Welcome` with the chosen mode and codec, or `Reject(BadDisplay)` when no mode fits.
    pub fn welcome(&self, hello: &ControlMessage, session: SessionId) -> ControlMessage {
        let bad_display = ControlMessage::Reject {
            reason: RejectReason::BadDisplay,
        };
        let ControlMessage::Hello {
            displays,
            stream_display,
            codecs,
            ..
        } = hello
        else {
            return bad_display;
        };
        match self.choose_mode(displays, *stream_display) {
            Some(mode) => ControlMessage::Welcome {
                session,
                mode,
                codec: self.choose_codec(codecs),
            },
            None => bad_display,
        }
    }

    /// The display at `index` if its size fits, else the fitting one with the most pixels (ties: higher
    /// refresh). A refresh above `max_fps` is capped to it: the stream runs at the host's limit.
    pub fn choose_mode(&self, displays: &[DisplayInfo], index: u8) -> Option<DisplayInfo> {
        let chosen = match displays.get(usize::from(index)) {
            Some(mode) if fits(mode) => Some(*mode),
            _ => displays
                .iter()
                .filter(|mode| fits(mode))
                .max_by_key(|mode| {
                    (
                        u64::from(mode.width) * u64::from(mode.height),
                        mode.refresh_mhz,
                    )
                })
                .copied(),
        };
        chosen.map(|mut mode| {
            mode.refresh_mhz = mode.refresh_mhz.min(self.max_fps.saturating_mul(1000));
            mode
        })
    }

    /// `Auto`: HEVC when offered, else H.264. A forced codec the client lacks falls back to H.264.
    pub fn choose_codec(&self, offered: &[CodecId]) -> CodecId {
        let wanted = match self.codec {
            CodecChoice::Auto | CodecChoice::Hevc => CodecId::Hevc,
            CodecChoice::H264 => CodecId::H264,
        };
        if offered.contains(&wanted) {
            wanted
        } else {
            CodecId::H264
        }
    }
}

/// Even sides (4:2:0 video) and at most 3840 x 2160.
fn fits(mode: &DisplayInfo) -> bool {
    mode.width.is_multiple_of(2)
        && mode.height.is_multiple_of(2)
        && mode.width <= MAX_WIDTH
        && mode.height <= MAX_HEIGHT
}

#[cfg(test)]
#[path = "mode_policy_tests.rs"]
mod tests;
