//! One reassembly slot: a preallocated message buffer plus a received-fragment bitmap.

use mixlink_core::ids::FrameId;

use crate::header::{Flags, MAX_PAYLOAD, PacketHeader};

/// What `Slot::insert` did with a fragment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Insert {
    Duplicate,
    Pending,
    /// Last missing fragment; the slot is free again and `data()` holds the message.
    Complete,
}

#[derive(Debug)]
pub(crate) struct Slot {
    pub(crate) busy: bool,
    pub(crate) frame: FrameId,
    pub(crate) flags: Flags,
    count: u16,
    received: u16,
    len: usize,
    bitmap: Vec<u64>,
    buf: Vec<u8>,
}

impl Slot {
    pub(crate) fn new(max_message_len: usize) -> Self {
        let max_frags = max_message_len.div_ceil(MAX_PAYLOAD).max(1);
        Self {
            busy: false,
            frame: FrameId::new(0),
            flags: Flags::empty(),
            count: 0,
            received: 0,
            len: 0,
            bitmap: vec![0; max_frags.div_ceil(64)],
            buf: vec![0; max_message_len],
        }
    }

    /// Takes the slot for the frame in `header`.
    pub(crate) fn start(&mut self, header: &PacketHeader) {
        self.busy = true;
        self.frame = header.frame;
        self.flags = header.flags;
        self.count = header.frag_count;
        self.received = 0;
        self.len = 0;
        self.bitmap.fill(0);
    }

    /// Same fragment count and flags as the fragment that started the slot.
    pub(crate) fn matches(&self, header: &PacketHeader) -> bool {
        self.count == header.frag_count && self.flags == header.flags
    }

    /// Copies one fragment in place. The caller has checked the fragment fits the buffer.
    // HOT PATH
    pub(crate) fn insert(&mut self, header: &PacketHeader, payload: &[u8]) -> Insert {
        let word = usize::from(header.frag_index / 64);
        let bit = 1u64 << (header.frag_index % 64);
        if self.bitmap[word] & bit != 0 {
            return Insert::Duplicate;
        }
        self.bitmap[word] |= bit;
        self.received += 1;
        let offset = usize::from(header.frag_index) * MAX_PAYLOAD;
        self.buf[offset..offset + payload.len()].copy_from_slice(payload);
        if header.frag_index + 1 == self.count {
            self.len = offset + payload.len();
        }
        if self.received < self.count {
            return Insert::Pending;
        }
        self.busy = false;
        Insert::Complete
    }

    pub(crate) fn data(&self) -> &[u8] {
        &self.buf[..self.len]
    }
}
