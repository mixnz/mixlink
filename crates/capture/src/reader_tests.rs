use super::*;
use crate::header::{FrameSlot, MAGIC, SLOT_COUNT, VERSION};

/// In-memory stand-in for the mapped driver header.
struct FakeHeader {
    magic: u32,
    version: u32,
    generation: u32,
    size: (u32, u32),
    latest_seq: u64,
    slots: [FrameSlot; SLOT_COUNT],
}

impl FakeHeader {
    fn new() -> Self {
        Self {
            magic: MAGIC,
            version: VERSION,
            generation: 1,
            size: (1920, 1080),
            latest_seq: 0,
            slots: [FrameSlot::default(); SLOT_COUNT],
        }
    }

    fn write(&mut self, seq: u64) {
        let slot = &mut self.slots[slot_index(seq)];
        slot.seq = seq;
        slot.surface_handle = 0x100 + seq;
        self.latest_seq = seq;
    }
}

impl HeaderView for FakeHeader {
    fn magic(&self) -> u32 {
        self.magic
    }
    fn version(&self) -> u32 {
        self.version
    }
    fn generation(&self) -> u32 {
        self.generation
    }
    fn size(&self) -> (u32, u32) {
        self.size
    }
    fn latest_seq(&self) -> u64 {
        self.latest_seq
    }
    fn slot(&self, index: usize) -> FrameSlot {
        self.slots[index]
    }
    fn slot_seq(&self, index: usize) -> u64 {
        self.slots[index].seq
    }
}

/// A reader that has already taken generation 1.
fn opened(h: &FakeHeader) -> ReaderState {
    let mut state = ReaderState::new();
    assert!(matches!(state.poll(h), Poll::Reopen { .. }));
    state
}

#[test]
fn first_poll_reopens_with_size() {
    let h = FakeHeader::new();
    let mut state = ReaderState::new();
    assert_eq!(
        state.poll(&h),
        Poll::Reopen {
            generation: 1,
            width: 1920,
            height: 1080
        }
    );
}

#[test]
fn new_seq_yields_latest_slot() {
    let mut h = FakeHeader::new();
    let mut state = opened(&h);
    h.write(5);
    h.write(6);
    match state.poll(&h) {
        Poll::Frame { seq, slot } => {
            assert_eq!(seq, 6);
            assert_eq!(slot.surface_handle, 0x106);
        }
        other => panic!("expected a frame, got {other:?}"),
    }
}

#[test]
fn same_seq_is_idle() {
    let mut h = FakeHeader::new();
    let mut state = opened(&h);
    h.write(1);
    assert!(matches!(state.poll(&h), Poll::Frame { seq: 1, .. }));
    assert_eq!(state.poll(&h), Poll::Idle);
}

#[test]
fn no_frame_yet_is_idle() {
    let h = FakeHeader::new();
    let mut state = opened(&h);
    assert_eq!(state.poll(&h), Poll::Idle);
}

#[test]
fn torn_slot_is_skipped() {
    let mut h = FakeHeader::new();
    let mut state = opened(&h);
    h.write(3);
    h.slots[slot_index(3)].seq = 0; // driver is rewriting the slot
    assert_eq!(state.poll(&h), Poll::Idle);
    h.slots[slot_index(3)].seq = 3;
    assert!(matches!(state.poll(&h), Poll::Frame { seq: 3, .. }));
}

#[test]
fn generation_change_reopens() {
    let mut h = FakeHeader::new();
    let mut state = opened(&h);
    h.write(7);
    h.generation = 2;
    h.size = (2560, 1440);
    assert_eq!(
        state.poll(&h),
        Poll::Reopen {
            generation: 2,
            width: 2560,
            height: 1440
        }
    );
    // Seqs keep growing across generations; the frame written before is still delivered.
    assert!(matches!(state.poll(&h), Poll::Frame { seq: 7, .. }));
}

#[test]
fn rejects_v2_header() {
    let mut h = FakeHeader::new();
    h.magic = 0x3253_584D; // "MXS2"
    h.version = 2;
    h.write(1);
    let mut state = ReaderState::new();
    assert_eq!(state.poll(&h), Poll::Gone);
    h.magic = MAGIC; // right magic, wrong version
    assert_eq!(state.poll(&h), Poll::Gone);
}

#[test]
fn magic_zero_is_gone_then_reopens() {
    let mut h = FakeHeader::new();
    let mut state = opened(&h);
    h.magic = 0;
    assert_eq!(state.poll(&h), Poll::Gone);
    h.magic = MAGIC; // same generation number published again: still a reopen
    assert!(matches!(state.poll(&h), Poll::Reopen { generation: 1, .. }));
}
