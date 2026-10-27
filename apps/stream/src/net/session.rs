//! Thread `stream-net` (spec 0020): the secure session with the host, then one loop over the control,
//! video and cursor lanes (`try_recv` + park, every lane wakes this thread). Video goes to the AU queue,
//! the cursor to `CursorTx`; keyframe and shape requests go back as feedback.

use std::net::SocketAddrV4;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use mixlink_codec::VideoCodec;
use mixlink_core::ipc::CloseReason;
use mixlink_net::control_link::{ControlLink, Wire};
use mixlink_net::error::NetError;
use mixlink_net::keys::ClientIdentity;
use mixlink_net::pairing::KnownHost;
use mixlink_net::session::client::{SecureSession, connect_secure};
use mixlink_net::{RxLane, Transport, TransportConfig, TxLane};
use mixlink_proto::channel::Channel;
use mixlink_proto::control::{ByeReason, CodecId, ControlMessage, DisplayInfo};

use super::drop_knob::DropKnob;
use super::feedback_tx::{FeedbackTx, Rtt};
use super::lanes::{Media, MediaCounts};
use super::reasons::{bye_reason, reject_reason};
use crate::au_queue::AuSender;
use crate::net::cursor_state::CursorTx;

/// No valid datagram from the host for this long: the connection is lost (as the host's timeout).
pub const SILENCE: Duration = Duration::from_secs(10);
const PING_EVERY: Duration = Duration::from_secs(1);
/// Longest park; any lane with data wakes the thread sooner.
const PARK: Duration = Duration::from_millis(5);

/// Who to connect to and how.
#[derive(Debug, Clone)]
pub struct SessionPlan {
    pub me: ClientIdentity,
    pub host: KnownHost,
    pub addr: SocketAddrV4,
    pub hello: ControlMessage,
    pub drop: DropKnob,
    pub silence: Duration,
}

/// Where the session's output goes.
#[derive(Debug)]
pub struct SessionIo {
    pub au: AuSender,
    pub cursor: CursorTx,
    /// Set from `Welcome` before any video, so the video thread opens the right decoder.
    pub codec: Arc<OnceLock<VideoCodec>>,
    /// The decoder failed: ask for a keyframe.
    pub want_keyframe: Arc<AtomicBool>,
    pub stop: Arc<AtomicBool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionEnd {
    /// `None`: stopped from this side (`stop`); `Bye` was sent.
    pub reason: Option<CloseReason>,
    pub counts: MediaCounts,
}

/// The session port's lanes on this side.
struct Link {
    _transport: Transport,
    ctrl_tx: TxLane,
    ctrl_rx: RxLane,
    video: RxLane,
    cursor: RxLane,
    feedback: TxLane,
}

fn open_link() -> Result<Link, CloseReason> {
    let error = |message: String| CloseReason::Error { message };
    let (transport, mut lanes) =
        Transport::start(TransportConfig::client()).map_err(|e| error(e.to_string()))?;
    let missing = || error("network lanes missing".to_owned());
    Ok(Link {
        ctrl_tx: lanes.take_tx(Channel::Control).ok_or_else(missing)?,
        ctrl_rx: lanes.take_rx(Channel::Control).ok_or_else(missing)?,
        video: lanes.take_rx(Channel::Video).ok_or_else(missing)?,
        cursor: lanes.take_rx(Channel::Cursor).ok_or_else(missing)?,
        feedback: lanes.take_tx(Channel::Feedback).ok_or_else(missing)?,
        _transport: transport,
    })
}

/// Connects and streams until the host ends, the link goes silent, or `stop`. Blocks.
pub fn run(
    plan: &SessionPlan,
    mut io: SessionIo,
    mut on_connected: impl FnMut(DisplayInfo, CodecId),
) -> SessionEnd {
    let ended = |reason| SessionEnd {
        reason,
        counts: MediaCounts::default(),
    };
    let mut link = match open_link() {
        Ok(link) => link,
        Err(reason) => return ended(Some(reason)),
    };
    let stop = Arc::clone(&io.stop);
    let mut wire = Wire {
        tx: &mut link.ctrl_tx,
        rx: &mut link.ctrl_rx,
        cancel: &stop,
    };
    let control = ControlLink::new(plan.addr);
    let mut session = match connect_secure(control, &mut wire, &plan.me, &plan.host, &plan.hello) {
        Ok(session) => session,
        Err(error) => return ended(connect_failed(error)),
    };
    let codec = match session.codec {
        CodecId::H264 => VideoCodec::H264,
        CodecId::Hevc => VideoCodec::Hevc,
    };
    let _ = io.codec.set(codec);
    on_connected(session.mode, session.codec);
    let media = Media::new(link.video, link.cursor, &session.keys, plan.addr, plan.drop);
    let feedback = FeedbackTx::new(link.feedback, &session.keys, plan.addr);
    let mut streaming = Streaming {
        session: &mut session,
        wire,
        media,
        feedback,
        rtt: Rtt::default(),
    };
    let reason = streaming.run(&mut io, plan.silence, &stop);
    SessionEnd {
        reason,
        counts: streaming.media.counts(),
    }
}

/// The loop after the handshake.
struct Streaming<'a, 'w> {
    session: &'a mut SecureSession,
    wire: Wire<'w>,
    media: Media,
    feedback: FeedbackTx,
    rtt: Rtt,
}

impl Streaming<'_, '_> {
    fn run(
        &mut self,
        io: &mut SessionIo,
        silence: Duration,
        stop: &AtomicBool,
    ) -> Option<CloseReason> {
        let (mut next_ping, mut ping_n) = (Instant::now(), 0u32);
        let mut last_valid = Instant::now();
        self.wire.rx.attach_current_thread();
        self.media.attach_current_thread();
        loop {
            let now = Instant::now();
            if stop.load(Ordering::Acquire) {
                if let Err(error) = self.session.bye(&mut self.wire) {
                    log::debug!("bye not sent: {error}");
                }
                return None;
            }
            let (heard, bye) = self.drain_control(now);
            if let Some(reason) = bye {
                return Some(CloseReason::HostEnded {
                    reason: bye_reason(reason).to_owned(),
                });
            }
            let media = self
                .media
                .pump(io, &mut self.feedback, self.rtt.one_way(), now);
            if heard || media {
                last_valid = now;
            }
            if now >= next_ping {
                self.rtt.sent(ping_n, now);
                let ping = ControlMessage::Ping { n: ping_n };
                if let Err(error) = self.session.link.send_sealed(self.wire.tx, &ping) {
                    log::debug!("ping not sent: {error}");
                }
                ping_n = ping_n.wrapping_add(1);
                next_ping = now + PING_EVERY;
            }
            if now.saturating_duration_since(last_valid) >= silence {
                return Some(CloseReason::ConnectionLost);
            }
            thread::park_timeout(PARK);
        }
    }

    /// Sealed control messages: pongs feed the RTT; a `Bye` ends the session.
    fn drain_control(&mut self, now: Instant) -> (bool, Option<ByeReason>) {
        let mut heard = false;
        while let Some(mut dg) = self.wire.rx.try_recv() {
            let Some(incoming) = self.session.link.accept(&mut dg) else {
                continue;
            };
            if !incoming.sealed {
                continue;
            }
            heard = true;
            match incoming.msg {
                ControlMessage::Pong { n } => self.rtt.pong(n, now),
                ControlMessage::Bye { reason } => return (true, Some(reason)),
                _ => {}
            }
        }
        (heard, None)
    }
}

/// `None`: cancelled from this side.
fn connect_failed(error: NetError) -> Option<CloseReason> {
    match error {
        NetError::Cancelled => None,
        NetError::Timeout => Some(CloseReason::HostUnreachable),
        NetError::Rejected(reason) => Some(reject_reason(reason)),
        other => Some(CloseReason::Error {
            message: other.to_string(),
        }),
    }
}

#[cfg(test)]
#[path = "session_tests.rs"]
mod tests;
