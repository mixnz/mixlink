//! Video channel message (spec 0020): a fixed 16-byte `FrameMeta` (little-endian) with the host's stage
//! times, then the encoded access unit. The client adds the network and its own stages without clock sync.

use crate::error::VideoError;

pub const FRAME_META_LEN: usize = 16;
const META_VERSION: u8 = 1;
const FLAG_KEYFRAME: u8 = 1 << 0;
const KNOWN_FLAGS: u8 = FLAG_KEYFRAME;

/// Host stage times of one frame, in microseconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameMeta {
    pub keyframe: bool,
    /// Driver acquire -> frame ready.
    pub capture_us: u32,
    pub encode_us: u32,
    /// Encode end -> last fragment queued.
    pub send_us: u32,
}

impl FrameMeta {
    // HOT PATH
    pub fn encode(&self, out: &mut [u8; FRAME_META_LEN]) {
        out[0] = META_VERSION;
        out[1] = if self.keyframe { FLAG_KEYFRAME } else { 0 };
        out[2..4].fill(0);
        out[4..8].copy_from_slice(&self.capture_us.to_le_bytes());
        out[8..12].copy_from_slice(&self.encode_us.to_le_bytes());
        out[12..16].copy_from_slice(&self.send_us.to_le_bytes());
    }

    /// Splits a video message into its meta and the access unit after it.
    // HOT PATH
    pub fn split(msg: &[u8]) -> Result<(FrameMeta, &[u8]), VideoError> {
        let Some((b, au)) = msg.split_first_chunk::<FRAME_META_LEN>() else {
            return Err(VideoError::TooShort { len: msg.len() });
        };
        if b[0] != META_VERSION {
            return Err(VideoError::Version { found: b[0] });
        }
        if b[1] & !KNOWN_FLAGS != 0 {
            return Err(VideoError::Flags { bits: b[1] });
        }
        let word = |at: usize| u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]]);
        let meta = FrameMeta {
            keyframe: b[1] & FLAG_KEYFRAME != 0,
            capture_us: word(4),
            encode_us: word(8),
            send_us: word(12),
        };
        Ok((meta, au))
    }
}

#[cfg(test)]
#[path = "video_tests.rs"]
mod tests;
