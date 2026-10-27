//! The media lanes of `stream-net` (spec 0020): video and cursor datagrams opened and turned into AUs
//! and cursor updates; keyframe and shape requests sent back.

use std::net::SocketAddrV4;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use mixlink_net::RxLane;
use mixlink_proto::channel::Channel;
use mixlink_proto::crypto::{Opener, Role, SessionKeys};
use mixlink_proto::cursor::{self, CursorMessage, ShapeKind};
use mixlink_proto::feedback::Feedback;
use mixlink_proto::header::PacketHeader;
use mixlink_proto::reassembler::{Push, Reassembler, ReassemblerConfig};
use mixlink_render::cursor::{CursorDraw, CursorKind, convert_shape};

use super::drop_knob::{DropKnob, DropLane};
use super::feedback_tx::FeedbackTx;
use super::receive::{ReceiveCounts, VideoReceiver};
use super::session::SessionIo;
use crate::au_queue::AuInfo;

/// Shortest time between two keyframe requests (the host forces at most one IDR per 50 ms).
pub const KEYFRAME_REQUEST_GAP: Duration = Duration::from_millis(100);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MediaCounts {
    pub video: ReceiveCounts,
    pub keyframe_requests: u64,
    pub shape_requests: u64,
    /// AUs dropped because the video thread was behind.
    pub queue_full: u64,
}

#[derive(Debug)]
pub struct Media {
    video: RxLane,
    cursor: RxLane,
    video_opener: Opener,
    cursor_opener: Opener,
    host: SocketAddrV4,
    drop: DropKnob,
    receiver: VideoReceiver,
    cursor_parts: Reassembler,
    last_keyframe_request: Option<Instant>,
    counts: MediaCounts,
}

impl Media {
    pub fn new(
        video: RxLane,
        cursor: RxLane,
        keys: &SessionKeys,
        host: SocketAddrV4,
        drop: DropKnob,
    ) -> Self {
        Self {
            video,
            cursor,
            video_opener: keys.opener(Role::Client, Channel::Video),
            cursor_opener: keys.opener(Role::Client, Channel::Cursor),
            host,
            drop,
            receiver: VideoReceiver::new(),
            cursor_parts: Reassembler::new(ReassemblerConfig::for_channel(Channel::Cursor)),
            last_keyframe_request: None,
            counts: MediaCounts::default(),
        }
    }

    pub fn attach_current_thread(&self) {
        self.video.attach_current_thread();
        self.cursor.attach_current_thread();
    }

    pub fn counts(&self) -> MediaCounts {
        MediaCounts {
            video: self.receiver.counts(),
            ..self.counts
        }
    }

    /// Drains both lanes; `true` when a valid datagram came from the host.
    // HOT PATH
    pub fn pump(
        &mut self,
        io: &mut SessionIo,
        feedback: &mut FeedbackTx,
        one_way: Duration,
        now: Instant,
    ) -> bool {
        let network_us = u32::try_from(one_way.as_micros()).unwrap_or(u32::MAX);
        let mut valid = false;
        while let Some(mut dg) = self.video.try_recv() {
            if dg.peer() != self.host || self.drop.should_drop(DropLane::Video) {
                continue;
            }
            let at = dg.at();
            let Ok(plain) = dg.open(&mut self.video_opener) else {
                continue;
            };
            valid = true;
            let Some(out) = self.receiver.push(plain, at) else {
                continue;
            };
            let info = AuInfo {
                first: out.first,
                last: out.last,
                host_us: [out.meta.capture_us, out.meta.encode_us, out.meta.send_us],
                network_us,
            };
            if io.au.send_with(out.au, info).is_err() {
                self.counts.queue_full += 1;
                self.receiver.lost_reference();
            }
        }
        if io.want_keyframe.swap(false, Ordering::AcqRel) {
            self.receiver.lost_reference();
        }
        self.ask_keyframe(feedback, now);
        valid |= self.pump_cursor(io, feedback, now);
        valid
    }

    fn ask_keyframe(&mut self, feedback: &mut FeedbackTx, now: Instant) {
        let due = self
            .last_keyframe_request
            .is_none_or(|at| now.saturating_duration_since(at) >= KEYFRAME_REQUEST_GAP);
        if self.receiver.needs_keyframe() && due {
            let last_good = self.receiver.last_good();
            if feedback.send(Feedback::KeyframeRequest { last_good }) {
                self.counts.keyframe_requests += 1;
                self.last_keyframe_request = Some(now);
            }
        }
    }

    // HOT PATH (positions); a shape is converted here, on change only.
    fn pump_cursor(&mut self, io: &mut SessionIo, feedback: &mut FeedbackTx, now: Instant) -> bool {
        let mut valid = false;
        while let Some(mut dg) = self.cursor.try_recv() {
            if dg.peer() != self.host || self.drop.should_drop(DropLane::Cursor) {
                continue;
            }
            let Ok(plain) = dg.open(&mut self.cursor_opener) else {
                continue;
            };
            valid = true;
            let Ok((header, payload)) = PacketHeader::decode(plain) else {
                continue;
            };
            let Push::Complete(msg) = self.cursor_parts.push(&header, payload) else {
                continue;
            };
            match cursor::decode(msg.data) {
                Ok(CursorMessage::Position(pos)) => {
                    let draw = CursorDraw {
                        x: pos.x,
                        y: pos.y,
                        visible: pos.visible,
                    };
                    if let Some(shape_id) = io.cursor.set_position(draw, pos.shape_id, now)
                        && feedback.send(Feedback::CursorShapeRequest { shape_id })
                    {
                        self.counts.shape_requests += 1;
                    }
                }
                Ok(CursorMessage::Shape { header, pixels }) => {
                    let kind = match header.kind {
                        ShapeKind::Color => CursorKind::Alpha,
                        ShapeKind::MaskedColor => CursorKind::MaskedColor,
                        // Only the DDA fallback sends these; not drawn yet.
                        ShapeKind::Monochrome => continue,
                    };
                    let size = (u32::from(header.width), u32::from(header.height));
                    let hot = (u32::from(header.hot_x), u32::from(header.hot_y));
                    match convert_shape(kind, size.0, size.1, hot, header.pitch, pixels) {
                        Ok(image) => io.cursor.push_shape(header.shape_id, image),
                        Err(error) => log::debug!("cursor shape dropped: {error}"),
                    }
                }
                Err(error) => log::trace!("cursor message dropped: {error}"),
            }
        }
        valid
    }
}
