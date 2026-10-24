//! Datagrams -> messages. One reassembler per channel and peer, owned by one thread; every buffer
//! is allocated in `new`, nothing allocates in `push`.

use mixlink_core::ids::{FrameId, Seq};

use crate::channel::Channel;
use crate::header::{Flags, MAX_PAYLOAD, PacketHeader};

/// Slot count and largest message for one channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReassemblerConfig {
    pub slots: usize,
    pub max_message_len: usize,
}

impl ReassemblerConfig {
    /// Defaults from spec 0010 (Channels table).
    pub const fn for_channel(channel: Channel) -> Self {
        let (slots, max_message_len) = match channel {
            Channel::Control => (2, 65_536),
            Channel::Video => (4, 4_194_304),
            Channel::Audio => (2, MAX_PAYLOAD),
            // 256x256 BGRA shape + its 18-byte header, rounded up.
            Channel::Cursor => (2, 270_336),
            Channel::Input => (2, MAX_PAYLOAD),
            Channel::Feedback => (2, MAX_PAYLOAD),
        };
        Self {
            slots,
            max_message_len,
        }
    }
}

/// A whole message, borrowed from the datagram (single packet) or from a slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Message<'a> {
    pub frame: FrameId,
    pub flags: Flags,
    pub data: &'a [u8],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Push<'a> {
    Complete(Message<'a>),
    Pending,
    Dropped(DropReason),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropReason {
    /// Frame is not newer than the last completed or evicted frame.
    Late,
    Duplicate,
    /// Inconsistent with earlier fragments, or larger than `max_message_len`.
    Malformed,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReassemblerStats {
    pub completed: u64,
    /// Frames given up: no free slot, or a newer frame completed first.
    pub evicted: u64,
    pub late: u64,
    pub duplicate: u64,
    pub malformed: u64,
    /// Sequence numbers skipped when first seen; reordered packets do not lower it.
    pub seq_gaps: u64,
}

#[derive(Debug)]
struct Slot {
    busy: bool,
    frame: FrameId,
    flags: Flags,
    count: u16,
    received: u16,
    len: usize,
    bitmap: Vec<u64>,
    buf: Vec<u8>,
}

#[derive(Debug)]
pub struct Reassembler {
    max_message_len: usize,
    slots: Vec<Slot>,
    /// Newest frame completed or evicted; frames not newer than it are late.
    floor: Option<FrameId>,
    last_seq: Option<Seq>,
    stats: ReassemblerStats,
}

impl Reassembler {
    pub fn new(config: ReassemblerConfig) -> Self {
        let max_frags = config.max_message_len.div_ceil(MAX_PAYLOAD).max(1);
        let slots = (0..config.slots.max(1))
            .map(|_| Slot {
                busy: false,
                frame: FrameId::new(0),
                flags: Flags::empty(),
                count: 0,
                received: 0,
                len: 0,
                bitmap: vec![0; max_frags.div_ceil(64)],
                buf: vec![0; config.max_message_len],
            })
            .collect();
        Self {
            max_message_len: config.max_message_len,
            slots,
            floor: None,
            last_seq: None,
            stats: ReassemblerStats::default(),
        }
    }

    pub const fn stats(&self) -> ReassemblerStats {
        self.stats
    }

    /// Adds one decoded datagram; returns the message when its last missing fragment arrives.
    // HOT PATH
    pub fn push<'a>(&'a mut self, header: &PacketHeader, payload: &'a [u8]) -> Push<'a> {
        self.track_seq(header.seq);
        if self.floor.is_some_and(|f| !header.frame.is_newer_than(f)) {
            self.stats.late += 1;
            return Push::Dropped(DropReason::Late);
        }
        let is_last = header.frag_index + 1 == header.frag_count;
        // Smallest size the message can have; a non-last fragment only proves one more byte.
        let min_len = usize::from(header.frag_count - 1) * MAX_PAYLOAD
            + if is_last { payload.len() } else { 1 };
        if min_len > self.max_message_len {
            self.stats.malformed += 1;
            return Push::Dropped(DropReason::Malformed);
        }
        if header.frag_count == 1 {
            self.finish(header.frame);
            return Push::Complete(Message {
                frame: header.frame,
                flags: header.flags,
                data: payload,
            });
        }
        let index = match self.slot_for(header) {
            Ok(index) => index,
            Err(reason) => return Push::Dropped(reason),
        };
        let slot = &mut self.slots[index];
        let word = usize::from(header.frag_index / 64);
        let bit = 1u64 << (header.frag_index % 64);
        if slot.bitmap[word] & bit != 0 {
            self.stats.duplicate += 1;
            return Push::Dropped(DropReason::Duplicate);
        }
        slot.bitmap[word] |= bit;
        slot.received += 1;
        let offset = usize::from(header.frag_index) * MAX_PAYLOAD;
        slot.buf[offset..offset + payload.len()].copy_from_slice(payload);
        if is_last {
            slot.len = offset + payload.len();
        }
        if slot.received < slot.count {
            return Push::Pending;
        }
        slot.busy = false;
        let (frame, flags, len) = (slot.frame, slot.flags, slot.len);
        self.finish(frame);
        Push::Complete(Message {
            frame,
            flags,
            data: &self.slots[index].buf[..len],
        })
    }

    fn track_seq(&mut self, seq: Seq) {
        match self.last_seq {
            Some(last) if !seq.is_newer_than(last) => {}
            Some(last) => {
                self.stats.seq_gaps += u64::from(seq.get().wrapping_sub(last.get()) - 1);
                self.last_seq = Some(seq);
            }
            None => self.last_seq = Some(seq),
        }
    }

    /// Slot already holding this frame, else a free one, else the oldest one evicted.
    fn slot_for(&mut self, header: &PacketHeader) -> Result<usize, DropReason> {
        if let Some(index) = self
            .slots
            .iter()
            .position(|s| s.busy && s.frame == header.frame)
        {
            let slot = &self.slots[index];
            if slot.count != header.frag_count || slot.flags != header.flags {
                self.stats.malformed += 1;
                return Err(DropReason::Malformed);
            }
            return Ok(index);
        }
        let index = match self.slots.iter().position(|s| !s.busy) {
            Some(index) => index,
            None => {
                let oldest = self.oldest_busy();
                let evicted = self.slots[oldest].frame;
                self.slots[oldest].busy = false;
                self.stats.evicted += 1;
                self.raise_floor(evicted);
                oldest
            }
        };
        let slot = &mut self.slots[index];
        slot.busy = true;
        slot.frame = header.frame;
        slot.flags = header.flags;
        slot.count = header.frag_count;
        slot.received = 0;
        slot.len = 0;
        slot.bitmap.fill(0);
        Ok(index)
    }

    fn oldest_busy(&self) -> usize {
        let mut oldest = 0;
        for (i, slot) in self.slots.iter().enumerate() {
            if self.slots[oldest].frame.is_newer_than(slot.frame) {
                oldest = i;
            }
        }
        oldest
    }

    /// Frame completed: count it and give up every pending frame older than it.
    fn finish(&mut self, frame: FrameId) {
        self.stats.completed += 1;
        for slot in &mut self.slots {
            if slot.busy && frame.is_newer_than(slot.frame) {
                slot.busy = false;
                self.stats.evicted += 1;
            }
        }
        self.raise_floor(frame);
    }

    fn raise_floor(&mut self, frame: FrameId) {
        if self.floor.is_none_or(|f| frame.is_newer_than(f)) {
            self.floor = Some(frame);
        }
    }
}

#[cfg(test)]
#[path = "reassembler_tests.rs"]
mod tests;
