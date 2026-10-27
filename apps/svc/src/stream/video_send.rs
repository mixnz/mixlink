//! Video send (spec 0020): `FrameMeta` + access unit → packetizer → sealed datagrams on the Video lane.

use std::net::SocketAddrV4;

use mixlink_core::clock;
use mixlink_core::ids::FrameId;
use mixlink_net::TxLane;
use mixlink_proto::channel::Channel;
use mixlink_proto::crypto::{Role, Sealer, SessionKeys};
use mixlink_proto::header::Flags;
use mixlink_proto::packetizer::Packetizer;
use mixlink_proto::video::{FRAME_META_LEN, FrameMeta};

use crate::capture::flow::Delivery;

/// Largest video message: the client's video reassembler limit (spec 0010).
pub const VIDEO_MESSAGE_MAX: usize = 4 * 1024 * 1024;

#[derive(Debug)]
pub struct VideoSender {
    tx: TxLane,
    sealer: Sealer,
    peer: SocketAddrV4,
    packetizer: Packetizer,
    /// Preallocated message buffer: meta, then the AU.
    buf: Box<[u8]>,
    frame: FrameId,
    /// Encode end → last fragment queued of the previous frame, in µs; a frame cannot carry its own.
    last_send_us: u32,
}

impl VideoSender {
    pub fn new(tx: TxLane, keys: &SessionKeys, peer: SocketAddrV4) -> VideoSender {
        VideoSender {
            tx,
            sealer: keys.sealer(Role::Host, Channel::Video),
            peer,
            packetizer: Packetizer::new(Channel::Video),
            buf: vec![0u8; VIDEO_MESSAGE_MAX].into_boxed_slice(),
            frame: FrameId::new(0),
            last_send_us: 0,
        }
    }

    /// Queues one access unit. `meta.send_us` is replaced by the previous frame's send time.
    /// `Dropped` when the AU does not fit or the lane fills mid-AU (the client then asks for an IDR).
    // HOT PATH
    pub fn send_au(&mut self, meta: &FrameMeta, au: &[u8]) -> Delivery {
        let start = clock::now();
        let len = FRAME_META_LEN + au.len();
        let Some(message) = self.buf.get_mut(..len) else {
            return Delivery::Dropped(au.len());
        };
        let (head, body) = message.split_at_mut(FRAME_META_LEN);
        let mut stamped = *meta;
        stamped.send_us = self.last_send_us;
        if let Ok(head) = <&mut [u8; FRAME_META_LEN]>::try_from(head) {
            stamped.encode(head);
        }
        body.copy_from_slice(au);
        let flags = if meta.keyframe {
            Flags::KEYFRAME
        } else {
            Flags::empty()
        };
        let frame = self.frame;
        self.frame = frame.next();
        let Ok(mut fragments) = self.packetizer.packetize(frame, flags, &self.buf[..len]) else {
            return Delivery::Dropped(au.len());
        };
        for _ in 0..fragments.count() {
            let Some(mut slot) = self.tx.try_reserve() else {
                return Delivery::Dropped(au.len());
            };
            let Some(datagram_len) = fragments.next_into(slot.datagram()) else {
                break;
            };
            if slot
                .seal_commit(&mut self.sealer, datagram_len, self.peer)
                .is_err()
            {
                return Delivery::Dropped(au.len());
            }
        }
        let elapsed = clock::now().duration_since(start);
        self.last_send_us = u32::try_from(elapsed.as_micros()).unwrap_or(u32::MAX);
        Delivery::Sent(au.len())
    }

    pub fn into_lane(self) -> TxLane {
        self.tx
    }
}

#[cfg(test)]
#[path = "video_send_tests.rs"]
pub(crate) mod tests;
