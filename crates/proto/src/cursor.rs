//! Cursor channel messages, fixed little-endian layout (ADR-0006: the client draws the cursor).
//! Byte 0 is the kind: 1 = position (one packet), 2 = shape (fragmented, sent on change).

use crate::error::CursorError;

const KIND_POSITION: u8 = 1;
const KIND_SHAPE: u8 = 2;
/// kind, x, y, visible, shape_id.
pub const CURSOR_POSITION_LEN: usize = 14;
/// kind, shape_id, shape kind, width, height, hot_x, hot_y, pitch; pixels follow.
pub const CURSOR_SHAPE_HEADER_LEN: usize = 18;

/// Hotspot position in virtual-monitor pixels and the shape it uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CursorPosition {
    pub x: i32,
    pub y: i32,
    pub visible: bool,
    /// A shape the client has not received means that shape was lost (recovery: not built yet).
    pub shape_id: u32,
}

impl CursorPosition {
    // HOT PATH
    pub fn encode(&self, out: &mut [u8; CURSOR_POSITION_LEN]) {
        out[0] = KIND_POSITION;
        out[1..5].copy_from_slice(&self.x.to_le_bytes());
        out[5..9].copy_from_slice(&self.y.to_le_bytes());
        out[9] = u8::from(self.visible);
        out[10..14].copy_from_slice(&self.shape_id.to_le_bytes());
    }
}

/// IddCx gives alpha (`Color`) and masked-color shapes; the DDA fallback adds `Monochrome`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum ShapeKind {
    Color = 1,
    MaskedColor = 2,
    /// AND mask over XOR mask: `height` counts both, so it is even.
    Monochrome = 3,
}

impl ShapeKind {
    const fn from_byte(byte: u8) -> Option<ShapeKind> {
        match byte {
            1 => Some(ShapeKind::Color),
            2 => Some(ShapeKind::MaskedColor),
            3 => Some(ShapeKind::Monochrome),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CursorShapeHeader {
    pub shape_id: u32,
    pub kind: ShapeKind,
    pub width: u16,
    pub height: u16,
    pub hot_x: u16,
    pub hot_y: u16,
    /// Bytes per pixel row; pixel data is `pitch * height` bytes.
    pub pitch: u32,
}

impl CursorShapeHeader {
    /// Writes the shape message into `out`; returns its length.
    pub fn encode_into(&self, pixels: &[u8], out: &mut [u8]) -> Result<usize, CursorError> {
        self.check_pixels(pixels.len())?;
        let need = CURSOR_SHAPE_HEADER_LEN + pixels.len();
        let have = out.len();
        let Some(out) = out.get_mut(..need) else {
            return Err(CursorError::BufferTooSmall { need, have });
        };
        out[0] = KIND_SHAPE;
        out[1..5].copy_from_slice(&self.shape_id.to_le_bytes());
        out[5] = self.kind as u8;
        out[6..8].copy_from_slice(&self.width.to_le_bytes());
        out[8..10].copy_from_slice(&self.height.to_le_bytes());
        out[10..12].copy_from_slice(&self.hot_x.to_le_bytes());
        out[12..14].copy_from_slice(&self.hot_y.to_le_bytes());
        out[14..18].copy_from_slice(&self.pitch.to_le_bytes());
        out[CURSOR_SHAPE_HEADER_LEN..].copy_from_slice(pixels);
        Ok(need)
    }

    fn check_pixels(&self, found: usize) -> Result<(), CursorError> {
        if self.kind == ShapeKind::Monochrome && !self.height.is_multiple_of(2) {
            return Err(CursorError::MonochromeHeight {
                height: self.height,
            });
        }
        let expected = u64::from(self.pitch) * u64::from(self.height);
        if usize::try_from(expected).ok() != Some(found) {
            return Err(CursorError::PixelLen {
                expected: usize::try_from(expected).unwrap_or(usize::MAX),
                found,
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorMessage<'a> {
    Position(CursorPosition),
    /// `pixels` borrows the decoded buffer.
    Shape {
        header: CursorShapeHeader,
        pixels: &'a [u8],
    },
}

/// Parses one cursor-channel message; never copies pixel data.
// HOT PATH
pub fn decode(bytes: &[u8]) -> Result<CursorMessage<'_>, CursorError> {
    let len = bytes.len();
    match bytes.first() {
        None => Err(CursorError::TooShort { len }),
        Some(&KIND_POSITION) => {
            let Some(b) = bytes.first_chunk::<CURSOR_POSITION_LEN>() else {
                return Err(CursorError::TooShort { len });
            };
            let visible = match b[9] {
                0 => false,
                1 => true,
                byte => return Err(CursorError::Visible { byte }),
            };
            Ok(CursorMessage::Position(CursorPosition {
                x: i32::from_le_bytes([b[1], b[2], b[3], b[4]]),
                y: i32::from_le_bytes([b[5], b[6], b[7], b[8]]),
                visible,
                shape_id: u32::from_le_bytes([b[10], b[11], b[12], b[13]]),
            }))
        }
        Some(&KIND_SHAPE) => {
            let Some((b, pixels)) = bytes.split_first_chunk::<CURSOR_SHAPE_HEADER_LEN>() else {
                return Err(CursorError::TooShort { len });
            };
            let kind = ShapeKind::from_byte(b[5]).ok_or(CursorError::ShapeKind { byte: b[5] })?;
            let header = CursorShapeHeader {
                shape_id: u32::from_le_bytes([b[1], b[2], b[3], b[4]]),
                kind,
                width: u16::from_le_bytes([b[6], b[7]]),
                height: u16::from_le_bytes([b[8], b[9]]),
                hot_x: u16::from_le_bytes([b[10], b[11]]),
                hot_y: u16::from_le_bytes([b[12], b[13]]),
                pitch: u32::from_le_bytes([b[14], b[15], b[16], b[17]]),
            };
            header.check_pixels(pixels.len())?;
            Ok(CursorMessage::Shape { header, pixels })
        }
        Some(&byte) => Err(CursorError::Kind { byte }),
    }
}

#[cfg(test)]
#[path = "cursor_tests.rs"]
mod tests;
