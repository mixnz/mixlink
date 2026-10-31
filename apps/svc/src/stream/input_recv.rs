//! One input session (spec 0021), run by `svc-input` (`input_service`): the client's keyboard and mouse
//! on the sealed Input lane, applied to the virtual HID driver. Keys held when the client goes quiet, or
//! when the session stops, are released, so nothing stays down on the host.

use std::fmt::Display;
use std::net::SocketAddrV4;
use std::thread;
use std::time::{Duration, Instant};

use mixlink_core::clock;
use mixlink_input::hid::report::ReportSink;
use mixlink_input::host::abs_map::AbsMap;
use mixlink_input::host::apply::InputApplier;
use mixlink_net::RxLane;
use mixlink_proto::channel::Channel;
use mixlink_proto::crypto::{Opener, Role, SessionKeys};
use mixlink_proto::header::PacketHeader;
use mixlink_proto::input::decode;

use crate::capture::report::Samples;

/// Held keys are released after this long without a packet (the client sends every 100 ms while
/// anything is held).
pub const SILENCE: Duration = Duration::from_millis(500);
/// One wait on the lane; also how often stop, commands and silence are checked.
pub const WAIT: Duration = Duration::from_millis(50);
/// Apply times kept for the session log; later ones are only counted.
const SAMPLES: usize = 72_000;

#[derive(Debug)]
pub struct InputRecv {
    opener: Opener,
    peer: SocketAddrV4,
    applier: InputApplier,
    last_packet: Option<Instant>,
    /// Receive → fed, in microseconds.
    apply_us: Samples,
    dropped: u64,
    feed_failed: bool,
}

impl InputRecv {
    pub fn new(keys: &SessionKeys, peer: SocketAddrV4, map: AbsMap) -> InputRecv {
        InputRecv {
            opener: keys.opener(Role::Host, Channel::Input),
            peer,
            applier: InputApplier::new(map),
            last_packet: None,
            apply_us: Samples::with_capacity(SAMPLES),
            dropped: 0,
            feed_failed: false,
        }
    }

    /// Applies the packets on `rx`; with none, parks up to `wait` (the lane, or a command, unparks this
    /// thread) and looks again. Then the silence check at `now`.
    // HOT PATH
    pub fn poll<S: ReportSink>(
        &mut self,
        rx: &mut RxLane,
        sink: &mut S,
        wait: Duration,
        now: Instant,
    ) where
        S::Error: Display,
    {
        if !self.drain(rx, sink, now) {
            thread::park_timeout(wait);
            self.drain(rx, sink, now);
        }
        let quiet = self
            .last_packet
            .is_none_or(|last| now.saturating_duration_since(last) >= SILENCE);
        if quiet && self.applier.held() {
            let released = self.applier.release_all(sink);
            self.note(released);
        }
    }

    /// Applies every queued packet; `false` when there was none.
    // HOT PATH
    fn drain<S: ReportSink>(&mut self, rx: &mut RxLane, sink: &mut S, now: Instant) -> bool
    where
        S::Error: Display,
    {
        let mut any = false;
        while let Some(mut dg) = rx.try_recv() {
            any = true;
            if dg.peer() != self.peer {
                self.dropped += 1;
                continue;
            }
            let at = dg.at();
            let Ok(plain) = dg.open(&mut self.opener) else {
                self.dropped += 1;
                continue;
            };
            let Ok((_, body)) = PacketHeader::decode(plain) else {
                self.dropped += 1;
                continue;
            };
            let Ok(packet) = decode(body) else {
                self.dropped += 1;
                continue;
            };
            self.last_packet = Some(now);
            let applied = self.applier.apply(&packet, sink);
            self.note(applied.map(|_| ()));
            let took = clock::now().duration_since(at);
            self.apply_us
                .push(u64::try_from(took.as_micros()).unwrap_or(u64::MAX));
        }
        any
    }

    /// Releases every key and button and logs the session's input numbers.
    pub fn finish<S: ReportSink>(&mut self, sink: &mut S)
    where
        S::Error: Display,
    {
        let released = self.applier.release_all(sink);
        self.note(released);
        let counts = self.applier.counts();
        log::info!(
            "input: {} packets, {} repeats or late, {} events recovered, {} bitmap fixes, {} dropped; \
             receive to fed p50/p99/max {}/{}/{} us",
            counts.packets,
            counts.stale,
            counts.recovered_events,
            counts.bitmap_fixes,
            self.dropped,
            self.apply_us.percentile(50.0),
            self.apply_us.percentile(99.0),
            self.apply_us.max()
        );
        log::info!(
            "input mouse: {} absolute and {} relative reports, {} presses, {} releases, {} mode changes; \
             position at the last releases {:?}",
            counts.abs_reports,
            counts.rel_reports,
            counts.button_downs,
            counts.button_ups,
            counts.mode_changes,
            counts.ups_at
        );
    }

    /// The first feed error is logged; the session keeps going (the driver may come back).
    fn note<E: Display>(&mut self, result: Result<(), E>) {
        if let Err(error) = result
            && !self.feed_failed
        {
            self.feed_failed = true;
            log::warn!("client input not fed to the virtual input driver: {error}");
        }
    }
}

#[cfg(test)]
#[path = "input_recv_tests.rs"]
mod tests;
