//! One stream at a time (spec 0020): the thread that owns the media lanes for a session, and the
//! control that starts it, stops it, and starts the next one only once the lanes are back.

use std::io;
use std::net::SocketAddrV4;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::thread::{self, JoinHandle};

use mixlink_core::config::VideoConfig;
use mixlink_core::ids::DeviceId;
use mixlink_net::{RxLane, TxLane};
use mixlink_proto::control::{CodecId, DisplayInfo};
use mixlink_proto::crypto::SessionKeys;

use crate::display::requests::{DisplayHandle, StreamTarget};
use crate::display::session::Active;
use crate::error::SvcError;

/// The session port's media lanes; one stream holds them at a time.
#[derive(Debug)]
pub struct StreamLanes {
    pub video: TxLane,
    pub cursor: TxLane,
    pub feedback: RxLane,
}

/// What a stream needs from `SessionStarted` and `host.json`.
#[derive(Debug, Clone)]
pub struct StreamStart {
    pub device: DeviceId,
    pub peer: SocketAddrV4,
    pub keys: SessionKeys,
    pub mode: DisplayInfo,
    pub codec: CodecId,
    pub video: VideoConfig,
}

#[derive(Debug)]
pub enum StreamEnd {
    Stopped,
    /// The session ends with `Bye { HostError }`.
    Failed(SvcError),
}

/// The client monitor, as the stream sees it (`DisplayHandle`; fakes in tests).
pub trait MonitorPort {
    fn begin(&self, client: DeviceId, mode: DisplayInfo) -> Result<StreamTarget, SvcError>;
    fn end(&self, active: Active) -> Result<(), SvcError>;
}

impl MonitorPort for DisplayHandle {
    fn begin(&self, client: DeviceId, mode: DisplayInfo) -> Result<StreamTarget, SvcError> {
        DisplayHandle::begin(self, client, mode)
    }

    fn end(&self, active: Active) -> Result<(), SvcError> {
        DisplayHandle::end(self, active)
    }
}

/// Monitor on, `capture` on its connector until it returns, monitor off. Lanes always come back.
pub fn run_session<M: MonitorPort>(
    monitor: &M,
    start: &StreamStart,
    lanes: StreamLanes,
    capture: impl FnOnce(u32, StreamLanes) -> (StreamLanes, Result<(), SvcError>),
) -> (StreamLanes, StreamEnd) {
    let target = match monitor.begin(start.device, start.mode) {
        Ok(target) => target,
        Err(error) => return (lanes, StreamEnd::Failed(error)),
    };
    let (lanes, captured) = capture(target.connector, lanes);
    if let Err(error) = monitor.end(target.active) {
        log::warn!("client monitor not removed cleanly: {error}");
    }
    let end = match captured {
        Ok(()) => StreamEnd::Stopped,
        Err(error) => StreamEnd::Failed(error),
    };
    (lanes, end)
}

/// The `svc-video` thread of one session.
#[derive(Debug)]
pub struct Streamer {
    stop: Arc<AtomicBool>,
    done: Receiver<(StreamLanes, StreamEnd)>,
    thread: Option<JoinHandle<()>>,
}

impl Streamer {
    /// Runs `body` on thread `svc-video`; it must return when the flag is set.
    pub fn spawn(
        lanes: StreamLanes,
        body: impl FnOnce(StreamLanes, &AtomicBool) -> (StreamLanes, StreamEnd) + Send + 'static,
    ) -> io::Result<Streamer> {
        let stop = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&stop);
        let (tx, done) = mpsc::channel();
        let thread = thread::Builder::new()
            .name("svc-video".into())
            .spawn(move || {
                // The receiver is gone only when the service stops; nothing to hand back then.
                let _ = tx.send(body(lanes, &flag));
            })?;
        Ok(Streamer {
            stop,
            done,
            thread: Some(thread),
        })
    }

    pub fn stop(&self) {
        self.stop.store(true, Ordering::Release);
    }

    /// The lanes and how it ended, once the thread finished; joins it.
    pub fn poll_done(&mut self) -> Option<(StreamLanes, StreamEnd)> {
        let done = self.done.try_recv().ok()?;
        self.join();
        Some(done)
    }

    /// Stops and waits (service stop).
    pub fn finish(mut self) -> Option<(StreamLanes, StreamEnd)> {
        self.stop();
        let done = self.done.recv().ok();
        self.join();
        done
    }

    fn join(&mut self) {
        if let Some(thread) = self.thread.take()
            && thread.join().is_err()
        {
            log::error!("video thread panicked");
        }
    }
}

/// Starts a stream per session; a new session waits until the old stream gave the lanes back.
#[derive(Debug)]
pub struct StreamControl<L> {
    lanes: Option<StreamLanes>,
    running: Option<(DeviceId, Streamer)>,
    pending: Option<StreamStart>,
    launch: L,
}

impl<L: FnMut(StreamStart, StreamLanes) -> io::Result<Streamer>> StreamControl<L> {
    pub fn new(lanes: StreamLanes, launch: L) -> Self {
        Self {
            lanes: Some(lanes),
            running: None,
            pending: None,
            launch,
        }
    }

    /// A session started: stream now, or after the running stream stopped.
    pub fn started(&mut self, start: StreamStart) {
        if let Some((_, streamer)) = &self.running {
            streamer.stop();
        }
        self.pending = Some(start);
        self.launch_pending();
    }

    pub fn ended(&mut self, device: DeviceId) {
        if let Some((running, streamer)) = &self.running
            && *running == device
        {
            streamer.stop();
        }
        if self.pending.as_ref().is_some_and(|p| p.device == device) {
            self.pending = None;
        }
    }

    /// Takes the lanes back from a finished stream and starts a waiting one. Returns how the finished
    /// one ended.
    pub fn poll(&mut self) -> Option<(DeviceId, StreamEnd)> {
        let (device, streamer) = self.running.as_mut()?;
        let (lanes, end) = streamer.poll_done()?;
        let device = *device;
        self.running = None;
        self.lanes = Some(lanes);
        self.launch_pending();
        Some((device, end))
    }

    /// Service stop: stops the running stream and waits for it.
    pub fn shutdown(&mut self) {
        self.pending = None;
        if let Some((_, streamer)) = self.running.take()
            && let Some((lanes, _)) = streamer.finish()
        {
            self.lanes = Some(lanes);
        }
    }

    fn launch_pending(&mut self) {
        if self.running.is_some() || self.pending.is_none() || self.lanes.is_none() {
            return;
        }
        let (Some(start), Some(lanes)) = (self.pending.take(), self.lanes.take()) else {
            return;
        };
        let device = start.device;
        match (self.launch)(start, lanes) {
            Ok(streamer) => self.running = Some((device, streamer)),
            // The lanes went with the closure: no more streams until the service restarts.
            Err(error) => log::error!("video thread not started, streaming off: {error}"),
        }
    }
}

#[cfg(test)]
#[path = "streamer_tests.rs"]
mod tests;
