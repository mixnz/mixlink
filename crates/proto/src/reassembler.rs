//! Datagrams -> messages. One reassembler per channel and peer, owned by one thread; every buffer
//! is allocated in `new`, nothing allocates in `push`.

use mixlink_core::ids::{FrameId, Seq};

use crate::channel::Channel;
use crate::header::{Flags, MAX_PAYLOAD, PacketHeader};
use crate::slot::{Insert, Slot};

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
        let slots = (0..config.slots.max(1))
            .map(|_| Slot::new(config.max_message_len))
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
        // `PacketHeader::decode` guarantees these; a hand-built header must not panic or overrun.
        let is_last = header.frag_index.checked_add(1) == Some(header.frag_count);
        if header.frag_index >= header.frag_count
            || payload.len() > MAX_PAYLOAD
            || (!is_last && payload.len() != MAX_PAYLOAD)
        {
            self.stats.malformed += 1;
            return Push::Dropped(DropReason::Malformed);
        }
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
        match self.slots[index].insert(header, payload) {
            Insert::Duplicate => {
                self.stats.duplicate += 1;
                Push::Dropped(DropReason::Duplicate)
            }
            Insert::Pending => Push::Pending,
            Insert::Complete => {
                self.finish(header.frame);
                let slot = &self.slots[index];
                Push::Complete(Message {
                    frame: slot.frame,
                    flags: slot.flags,
                    data: slot.data(),
                })
            }
        }
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
            if !self.slots[index].matches(header) {
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
                // Reordered frame older than everything pending: drop it, keep the newer ones.
                if !header.frame.is_newer_than(evicted) {
                    self.stats.late += 1;
                    return Err(DropReason::Late);
                }
                self.slots[oldest].busy = false;
                self.stats.evicted += 1;
                self.raise_floor(evicted);
                oldest
            }
        };
        self.slots[index].start(header);
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

    /// Frame completed: count it and give up every pending frame not newer than it (the same frame
    /// can still hold a slot when a single packet completed it).
    fn finish(&mut self, frame: FrameId) {
        self.stats.completed += 1;
        for slot in &mut self.slots {
            if slot.busy && !slot.frame.is_newer_than(frame) {
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

#[cfg(test)]
#[path = "reassembler_stress_tests.rs"]
mod stress_tests;
