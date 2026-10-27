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
    /// `video.max_fps`; a mode's refresh rounded up to whole Hz must not exceed it.
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

    /// The display at `index` if it fits, else the fitting one with the most pixels (ties: higher refresh).
    pub fn choose_mode(&self, displays: &[DisplayInfo], index: u8) -> Option<DisplayInfo> {
        if let Some(mode) = displays.get(usize::from(index))
            && self.fits(mode)
        {
            return Some(*mode);
        }
        displays
            .iter()
            .filter(|mode| self.fits(mode))
            .max_by_key(|mode| {
                (
                    u64::from(mode.width) * u64::from(mode.height),
                    mode.refresh_mhz,
                )
            })
            .copied()
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

    fn fits(&self, mode: &DisplayInfo) -> bool {
        mode.width.is_multiple_of(2)
            && mode.height.is_multiple_of(2)
            && mode.width <= MAX_WIDTH
            && mode.height <= MAX_HEIGHT
            && mode.refresh_mhz.div_ceil(1000) <= self.max_fps
    }
}

#[cfg(test)]
#[path = "mode_policy_tests.rs"]
mod tests;
