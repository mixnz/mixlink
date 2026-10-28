//! Feedback from the session's client (spec 0020): keyframe and cursor shape requests on the sealed
//! Feedback lane, drained once per video frame. Forced IDRs are at most one per `IDR_GAP`.

use std::net::SocketAddrV4;
use std::time::{Duration, Instant};

use mixlink_net::RxLane;
use mixlink_proto::channel::Channel;
use mixlink_proto::crypto::{Opener, Role, SessionKeys};
use mixlink_proto::feedback::Feedback;
use mixlink_proto::header::PacketHeader;

/// Shortest time between two IDRs forced by the client: each one is large, a burst of requests
/// after one loss must not become a burst of IDRs.
pub const IDR_GAP: Duration = Duration::from_millis(50);

/// What the stream must do after a drain.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FeedbackPoll {
    pub force_idr: bool,
    pub shape_request: bool,
}

#[derive(Debug)]
pub struct FeedbackRx {
    rx: RxLane,
    opener: Opener,
    peer: SocketAddrV4,
    idr_pending: bool,
    last_idr: Option<Instant>,
}

impl FeedbackRx {
    pub fn new(rx: RxLane, keys: &SessionKeys, peer: SocketAddrV4) -> FeedbackRx {
        FeedbackRx {
            rx,
            opener: keys.opener(Role::Host, Channel::Feedback),
            peer,
            idr_pending: false,
            last_idr: None,
        }
    }

    /// Drains the lane without waiting. Datagrams from another address, not sealed for this session,
    /// or of an unknown kind are dropped.
    // HOT PATH
    pub fn poll(&mut self, now: Instant) -> FeedbackPoll {
        let mut out = FeedbackPoll::default();
        while let Some(mut dg) = self.rx.try_recv() {
            if dg.peer() != self.peer {
                continue;
            }
            let Ok(plain) = dg.open(&mut self.opener) else {
                continue;
            };
            let Ok((_, payload)) = PacketHeader::decode(plain) else {
                continue;
            };
            match Feedback::decode(payload) {
                Ok(Feedback::KeyframeRequest { .. }) => self.idr_pending = true,
                Ok(Feedback::CursorShapeRequest { .. }) => out.shape_request = true,
                Err(_) => {}
            }
        }
        let gap_over = self
            .last_idr
            .is_none_or(|last| now.saturating_duration_since(last) >= IDR_GAP);
        if self.idr_pending && gap_over {
            self.idr_pending = false;
            self.last_idr = Some(now);
            out.force_idr = true;
        }
        out
    }

    pub fn into_lane(self) -> RxLane {
        self.rx
    }
}

#[cfg(test)]
#[path = "feedback_tests.rs"]
mod tests;
