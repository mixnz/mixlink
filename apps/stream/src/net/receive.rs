//! Video receive (spec 0020): opened datagrams → reassembled messages → `FrameMeta` + AU. A frame that
//! is given up or skipped means the decoder lacks a reference: non-key frames are dropped until a
//! keyframe arrives, and the session asks the host for one.

use mixlink_core::clock::Ticks;
use mixlink_core::ids::FrameId;
use mixlink_proto::channel::Channel;
use mixlink_proto::header::{Flags, PacketHeader};
use mixlink_proto::reassembler::{Push, Reassembler, ReassemblerConfig};
use mixlink_proto::video::FrameMeta;

/// One complete access unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VideoOut<'a> {
    pub meta: FrameMeta,
    pub au: &'a [u8],
    /// First and last fragment arrival.
    pub first: Ticks,
    pub last: Ticks,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReceiveCounts {
    pub frames: u64,
    /// Given up by the reassembler or skipped.
    pub lost: u64,
    /// Complete but dropped while waiting for a keyframe.
    pub skipped: u64,
}

#[derive(Debug)]
pub struct VideoReceiver {
    reassembler: Reassembler,
    last_complete: Option<FrameId>,
    waiting_key: bool,
    seen_any: bool,
    evicted_seen: u64,
    /// The frame whose first fragment came last, and when.
    first: (FrameId, Ticks),
    counts: ReceiveCounts,
}

impl Default for VideoReceiver {
    fn default() -> Self {
        Self::new()
    }
}

impl VideoReceiver {
    /// Allocates the 4 MiB reassembly slots now.
    pub fn new() -> Self {
        Self {
            reassembler: Reassembler::new(ReassemblerConfig::for_channel(Channel::Video)),
            last_complete: None,
            waiting_key: true,
            seen_any: false,
            evicted_seen: 0,
            first: (FrameId::new(0), Ticks::ZERO),
            counts: ReceiveCounts::default(),
        }
    }

    /// One opened datagram (header + payload) received at `at`.
    // HOT PATH
    pub fn push<'a>(&'a mut self, plain: &'a [u8], at: Ticks) -> Option<VideoOut<'a>> {
        let (header, payload) = PacketHeader::decode(plain).ok()?;
        self.seen_any = true;
        if self.first.0 != header.frame || self.first.1 == Ticks::ZERO {
            self.first = (header.frame, at);
        }
        let first = self.first;
        // Frames given up so far (seen one datagram late; a skipped frame id also catches it).
        let evicted = self.reassembler.stats().evicted;
        if evicted > self.evicted_seen {
            self.counts.lost += evicted - self.evicted_seen;
            self.evicted_seen = evicted;
            self.waiting_key = true;
        }
        let Push::Complete(msg) = self.reassembler.push(&header, payload) else {
            return None;
        };
        let keyframe = msg.flags.contains(Flags::KEYFRAME);
        if let Some(last) = self.last_complete
            && msg.frame != last.next()
        {
            self.counts.lost += 1;
            self.waiting_key = true;
        }
        self.last_complete = Some(msg.frame);
        if self.waiting_key && !keyframe {
            self.counts.skipped += 1;
            return None;
        }
        let (meta, au) = FrameMeta::split(msg.data).ok()?;
        self.waiting_key = false;
        self.counts.frames += 1;
        Some(VideoOut {
            meta,
            au,
            first: if first.0 == msg.frame { first.1 } else { at },
            last: at,
        })
    }

    /// The decoder failed: wait for a keyframe as after a loss.
    pub fn lost_reference(&mut self) {
        self.waiting_key = true;
    }

    /// A keyframe is needed (after some video arrived; the host sends one first anyway).
    pub fn needs_keyframe(&self) -> bool {
        self.waiting_key && self.seen_any
    }

    pub fn last_good(&self) -> FrameId {
        self.last_complete.unwrap_or(FrameId::new(0))
    }

    pub fn counts(&self) -> ReceiveCounts {
        self.counts
    }
}

#[cfg(test)]
#[path = "receive_tests.rs"]
mod tests;
