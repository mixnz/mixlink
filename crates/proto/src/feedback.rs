//! Feedback channel messages v1 (spec 0020), client -> host, one packet each, fixed little-endian layout.
//! Byte 0 is the kind: 1 = keyframe request, 2 = cursor shape request. FEC feedback adds kinds later.

use mixlink_core::ids::FrameId;

use crate::error::FeedbackError;

const KIND_KEYFRAME: u8 = 1;
const KIND_CURSOR_SHAPE: u8 = 2;
/// kind + one u32.
pub const FEEDBACK_LEN: usize = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Feedback {
    /// A frame was lost: the next one must be an IDR. `last_good` = newest frame decoded.
    KeyframeRequest { last_good: FrameId },
    /// A cursor position named a shape the client does not have.
    CursorShapeRequest { shape_id: u32 },
}

impl Feedback {
    /// Writes the message; returns its length.
    // HOT PATH
    pub fn encode(&self, out: &mut [u8; FEEDBACK_LEN]) -> usize {
        let (kind, value) = match *self {
            Feedback::KeyframeRequest { last_good } => (KIND_KEYFRAME, last_good.get()),
            Feedback::CursorShapeRequest { shape_id } => (KIND_CURSOR_SHAPE, shape_id),
        };
        out[0] = kind;
        out[1..5].copy_from_slice(&value.to_le_bytes());
        FEEDBACK_LEN
    }

    // HOT PATH
    pub fn decode(bytes: &[u8]) -> Result<Feedback, FeedbackError> {
        let Some(b) = bytes.first_chunk::<FEEDBACK_LEN>() else {
            return Err(FeedbackError::TooShort { len: bytes.len() });
        };
        let value = u32::from_le_bytes([b[1], b[2], b[3], b[4]]);
        match b[0] {
            KIND_KEYFRAME => Ok(Feedback::KeyframeRequest {
                last_good: FrameId::new(value),
            }),
            KIND_CURSOR_SHAPE => Ok(Feedback::CursorShapeRequest { shape_id: value }),
            byte => Err(FeedbackError::Kind { byte }),
        }
    }
}

#[cfg(test)]
#[path = "feedback_tests.rs"]
mod tests;
