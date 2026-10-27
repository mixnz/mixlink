//! Frame share header v3 written by `drivers/vdisplay/SharedHeader.h` (spec 0019), one per monitor
//! connector. Same layout as the C++ struct (`#pragma pack(8)`); change both together.

/// "MXF3" read as a little-endian u32.
pub const MAGIC: u32 = 0x3346_584D;
pub const VERSION: u32 = 3;
pub const SLOT_COUNT: usize = 4;
/// Connectors 0..CONNECTOR_COUNT (the driver's monitor limit).
pub const CONNECTOR_COUNT: u32 = 4;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FrameSlot {
    /// 0 while the driver writes the slot.
    pub seq: u64,
    pub present_frame_number: u32,
    pub dirty_rect_count: u32,
    /// QPC of the DWM present, in the clock `mixlink_core::clock` uses.
    pub present_qpc: u64,
    /// QPC when the driver acquired the surface.
    pub acquire_qpc: u64,
    /// Driver's NT handle of the IddCx surface itself; 0 = the frame is in ring texture `seq % SLOT_COUNT`.
    pub surface_handle: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct FrameHeader {
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
    /// Committed refresh in millihertz; 0 = unknown.
    pub refresh_mhz: u32,
    pub reserved: u32,
    pub texture_handles: [u64; SLOT_COUNT],
    pub fence_handle: u64,
    pub latest_seq: u64,
    pub slots: [FrameSlot; SLOT_COUNT],
    /// Reader: its process id while attached, else 0.
    pub reader_pid: u32,
    /// Driver: hold waits that hit the cap.
    pub hold_timeouts: u32,
    /// Reader: heartbeat, bumped on every wait.
    pub reader_seq: u64,
    /// Reader: last frame seq it is done with.
    pub released_seq: u64,
    /// Driver: swapchains that fell back to the ring copy.
    pub ring_fallbacks: u32,
    pub reserved2: u32,
}

/// Ring / slot index of a frame seq.
pub const fn slot_index(seq: u64) -> usize {
    (seq % SLOT_COUNT as u64) as usize
}

/// Shared section of one connector.
pub fn section_name(connector: u32) -> String {
    format!("Global\\MixLinkFrames-{connector}")
}

/// Auto-reset event the driver sets after each published frame.
pub fn ready_event_name(connector: u32) -> String {
    format!("Global\\MixLinkFrameReady-{connector}")
}

/// Auto-reset event the reader sets after writing `released_seq`.
pub fn released_event_name(connector: u32) -> String {
    format!("Global\\MixLinkFrameReleased-{connector}")
}

#[cfg(test)]
#[path = "header_tests.rs"]
mod tests;
