//! Cursor share v1 written by `drivers/vdisplay/CursorShare.h` (spec 0020), one per monitor connector:
//! a 64-byte seqlock header (little-endian u32 fields) followed by the shape pixels. Change both together.
//! The seqlock read is pure over `CursorView`, so tests run on plain memory.

/// "MXC1" read as a little-endian u32.
pub const MAGIC: u32 = 0x3143_584D;
pub const HEADER_LEN: usize = 64;
/// 256 x 256 BGRA, the size the driver offers IddCx.
pub const SHAPE_MAX_BYTES: usize = 256 * 256 * 4;
/// Whole section: header + pixel area.
pub const SECTION_LEN: usize = HEADER_LEN + SHAPE_MAX_BYTES;

pub const OFF_MAGIC: usize = 0;
pub const OFF_SEQ: usize = 4;
pub const OFF_VISIBLE: usize = 8;
pub const OFF_X: usize = 12;
pub const OFF_Y: usize = 16;
pub const OFF_SHAPE_ID: usize = 20;
pub const OFF_SHAPE_TYPE: usize = 24;
pub const OFF_WIDTH: usize = 28;
pub const OFF_HEIGHT: usize = 32;
pub const OFF_PITCH: usize = 36;
pub const OFF_XHOT: usize = 40;
pub const OFF_YHOT: usize = 44;
pub const OFF_SHAPE_BYTES: usize = 48;

/// IddCx `IDDCX_CURSOR_SHAPE_TYPE` values.
const TYPE_MASKED_COLOR: u32 = 1;
const TYPE_ALPHA: u32 = 2;

/// Pixel format of a shape (32 bpp BGRA either way).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShapeType {
    /// Straight alpha.
    Alpha,
    /// Alpha byte is a mask: 0 = opaque colour, 0xFF = XOR with the screen.
    MaskedColor,
}

/// Read access to a cursor section. `word` must be an acquire load (the seqlock relies on it).
pub trait CursorView {
    fn word(&self, offset: usize) -> u32;
    /// Copies the first `out.len()` pixel bytes.
    fn copy_pixels(&self, out: &mut [u8]);
}

/// Hotspot position in monitor pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CursorPos {
    pub x: i32,
    pub y: i32,
    pub visible: bool,
    /// IddCx shape id; 0 = no shape yet.
    pub shape_id: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShapeInfo {
    pub kind: ShapeType,
    pub width: u16,
    pub height: u16,
    pub hot_x: u16,
    pub hot_y: u16,
    pub pitch: u32,
    /// Pixel bytes copied out (`pitch * height`).
    pub len: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Snapshot {
    /// The driver wrote while we read, or the header is inconsistent: read again.
    Torn,
    /// No swapchain on this monitor (`Magic` 0).
    Gone,
    /// `shape` is set (pixels in `shape_out`) when the shape id differs from `last_shape`.
    Cursor {
        pos: CursorPos,
        shape: Option<ShapeInfo>,
    },
}

/// One seqlock read. `shape_out` must hold `SHAPE_MAX_BYTES`.
// HOT PATH
pub fn read_snapshot(view: &impl CursorView, shape_out: &mut [u8], last_shape: u32) -> Snapshot {
    let before = view.word(OFF_SEQ);
    if before % 2 == 1 {
        return Snapshot::Torn;
    }
    if view.word(OFF_MAGIC) != MAGIC {
        return Snapshot::Gone;
    }
    let shape_id = view.word(OFF_SHAPE_ID);
    let pos = CursorPos {
        x: as_i32(view.word(OFF_X)).saturating_add(as_i32(view.word(OFF_XHOT))),
        y: as_i32(view.word(OFF_Y)).saturating_add(as_i32(view.word(OFF_YHOT))),
        visible: view.word(OFF_VISIBLE) != 0,
        shape_id,
    };
    let shape = if shape_id != 0 && shape_id != last_shape {
        match read_shape(view, shape_out) {
            Some(info) => Some(info),
            None => return Snapshot::Torn,
        }
    } else {
        None
    };
    if view.word(OFF_SEQ) != before {
        return Snapshot::Torn;
    }
    Snapshot::Cursor { pos, shape }
}

fn read_shape(view: &impl CursorView, shape_out: &mut [u8]) -> Option<ShapeInfo> {
    let kind = match view.word(OFF_SHAPE_TYPE) {
        TYPE_MASKED_COLOR => ShapeType::MaskedColor,
        TYPE_ALPHA => ShapeType::Alpha,
        _ => return None,
    };
    let dim = |offset| u16::try_from(view.word(offset)).ok();
    let (width, height) = (dim(OFF_WIDTH)?, dim(OFF_HEIGHT)?);
    let pitch = view.word(OFF_PITCH);
    let len = usize::try_from(view.word(OFF_SHAPE_BYTES)).ok()?;
    let expected = usize::try_from(u64::from(pitch) * u64::from(height)).ok()?;
    if len != expected || len > SHAPE_MAX_BYTES {
        return None;
    }
    view.copy_pixels(shape_out.get_mut(..len)?);
    Some(ShapeInfo {
        kind,
        width,
        height,
        hot_x: dim(OFF_XHOT)?,
        hot_y: dim(OFF_YHOT)?,
        pitch,
        len,
    })
}

/// The driver writes `INT32` fields; same bits.
const fn as_i32(word: u32) -> i32 {
    i32::from_le_bytes(word.to_le_bytes())
}

/// Shared section of one connector.
pub fn cursor_section_name(connector: u32) -> String {
    format!("Global\\MixLinkCursor-{connector}")
}

/// Auto-reset event the driver sets after each cursor update.
pub fn cursor_event_name(connector: u32) -> String {
    format!("Global\\MixLinkCursorReady-{connector}")
}

#[cfg(test)]
#[path = "cursor_header_tests.rs"]
mod tests;
