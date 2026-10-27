use std::mem::{offset_of, size_of};

use super::*;

#[test]
fn layout_matches_driver() {
    // Values asserted in drivers/vdisplay/SharedHeader.h.
    assert_eq!(size_of::<FrameSlot>(), 40);
    assert_eq!(size_of::<FrameHeader>(), 288);
    assert_eq!(offset_of!(FrameHeader, refresh_mhz), 40);
    assert_eq!(offset_of!(FrameHeader, slots), 96);
    assert_eq!(offset_of!(FrameHeader, reader_pid), 256);
    assert_eq!(offset_of!(FrameHeader, reader_seq), 264);
    assert_eq!(offset_of!(FrameHeader, released_seq), 272);
    assert_eq!(offset_of!(FrameHeader, ring_fallbacks), 280);
}

#[test]
fn names_per_connector() {
    assert_eq!(section_name(2), "Global\\MixLinkFrames-2");
    assert_eq!(ready_event_name(0), "Global\\MixLinkFrameReady-0");
    assert_eq!(released_event_name(3), "Global\\MixLinkFrameReleased-3");
}

#[test]
fn magic_spells_mxf3() {
    assert_eq!(&MAGIC.to_le_bytes(), b"MXF3");
}
