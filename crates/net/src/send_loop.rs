//! Send thread: drains tx lanes in priority order onto the socket.

use std::net::UdpSocket;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::thread;

use mixlink_proto::channel::Channel;

use crate::lane::{Reader, Wake};
use crate::platform::Mmcss;
use crate::stats::SocketCounters;

/// Lanes drained fully on every pass, highest priority first; video goes last, in batches.
pub(crate) const PRIORITY: [Channel; 5] = [
    Channel::Control,
    Channel::Input,
    Channel::Feedback,
    Channel::Audio,
    Channel::Cursor,
];
/// Video datagrams sent per pass before the other lanes are checked again.
pub(crate) const VIDEO_BATCH: usize = 32;

pub(crate) struct SendLoop {
    pub(crate) socket: Arc<UdpSocket>,
    /// Same order as `PRIORITY`.
    pub(crate) lanes: [Reader; 5],
    pub(crate) video: Reader,
    pub(crate) wake: Arc<Wake>,
    pub(crate) stop: Arc<AtomicBool>,
    pub(crate) counters: Arc<SocketCounters>,
}

impl SendLoop {
    /// Thread body; reports MMCSS on `ready` once it can be woken.
    pub(crate) fn run(mut self, ready: Sender<bool>) {
        self.wake.attach_current();
        let mmcss = Mmcss::enter();
        let _ = ready.send(mmcss.is_some());
        loop {
            if !self.pass() {
                if self.stop.load(Ordering::Acquire) {
                    break;
                }
                thread::park();
            }
        }
        drop(mmcss);
    }

    // HOT PATH
    /// One pass over every lane; `true` if anything was sent.
    fn pass(&mut self) -> bool {
        let mut sent = false;
        for lane in &mut self.lanes {
            while let Some(datagram) = lane.try_pop() {
                if self
                    .socket
                    .send_to(datagram.data(), datagram.peer())
                    .is_err()
                {
                    SocketCounters::bump(&self.counters.send_err);
                }
                sent = true;
            }
        }
        for _ in 0..VIDEO_BATCH {
            let Some(datagram) = self.video.try_pop() else {
                break;
            };
            if self
                .socket
                .send_to(datagram.data(), datagram.peer())
                .is_err()
            {
                SocketCounters::bump(&self.counters.send_err);
            }
            sent = true;
        }
        sent
    }
}
