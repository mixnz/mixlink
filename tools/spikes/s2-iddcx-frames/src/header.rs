//! Shared-memory header written by `drivers/vdisplay/FrameShare.h`. Same layout, change both together.

use std::fmt;

pub const MAGIC: u32 = 0x3253_584D; // "MXS2"
pub const VERSION: u32 = 1;
pub const SLOT_COUNT: usize = 4;
pub const SECTION_NAME: &str = "Global\\MixLinkS2Frames";

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct S2Slot {
    /// 0 while the driver writes the slot.
    pub seq: u64,
    pub present_frame_number: u32,
    pub dirty_rect_count: u32,
    pub present_qpc: u64,
    pub acquire_qpc: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct S2Header {
    /// 0 while no swapchain is shared.
    pub magic: u32,
    pub version: u32,
    pub wudf_pid: u32,
    pub slot_count: u32,
    pub generation: u32,
    pub width: u32,
    pub height: u32,
    pub dxgi_format: u32,
    pub luid_low: u32,
    pub luid_high: i32,
    pub texture_handles: [u64; SLOT_COUNT],
    pub fence_handle: u64,
    pub latest_seq: u64,
    pub slots: [S2Slot; SLOT_COUNT],
}

impl S2Header {
    pub const fn zeroed() -> Self {
        const SLOT: S2Slot = S2Slot {
            seq: 0,
            present_frame_number: 0,
            dirty_rect_count: 0,
            present_qpc: 0,
            acquire_qpc: 0,
        };
        Self {
            magic: 0,
            version: 0,
            wudf_pid: 0,
            slot_count: 0,
            generation: 0,
            width: 0,
            height: 0,
            dxgi_format: 0,
            luid_low: 0,
            luid_high: 0,
            texture_handles: [0; SLOT_COUNT],
            fence_handle: 0,
            latest_seq: 0,
            slots: [SLOT; SLOT_COUNT],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeaderError {
    BadMagic,
    BadVersion(u32),
    BadSlotCount(u32),
}

impl fmt::Display for HeaderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadMagic => write!(f, "no frames shared yet (magic not set)"),
            Self::BadVersion(v) => write!(f, "header version {v}, expected {VERSION}"),
            Self::BadSlotCount(n) => write!(f, "slot count {n}, expected {SLOT_COUNT}"),
        }
    }
}

pub fn validate(h: &S2Header) -> Result<(), HeaderError> {
    if h.magic != MAGIC {
        return Err(HeaderError::BadMagic);
    }
    if h.version != VERSION {
        return Err(HeaderError::BadVersion(h.version));
    }
    if h.slot_count as usize != SLOT_COUNT {
        return Err(HeaderError::BadSlotCount(h.slot_count));
    }
    Ok(())
}

/// Tells when the driver reopened its ring (new swapchain, new mode).
#[derive(Debug, Default)]
pub struct GenerationWatch {
    last: Option<u32>,
}

impl GenerationWatch {
    pub fn changed(&mut self, generation: u32) -> bool {
        let changed = self.last != Some(generation);
        self.last = Some(generation);
        changed
    }
}

#[cfg(test)]
#[path = "header_tests.rs"]
mod tests;
