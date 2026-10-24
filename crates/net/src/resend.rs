//! "Resend every `interval` until a reply or `give_up`": pure logic, the caller owns the clock.

use std::time::Duration;

use mixlink_core::clock::Ticks;

/// Client `Hello` resend interval.
pub const HELLO_RESEND: Duration = Duration::from_millis(250);
/// Client stops resending `Hello` after this long without a reply.
pub const HELLO_GIVE_UP: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResendAction {
    /// Send (again) now.
    Send,
    /// Nothing to do for this long; wait for a reply meanwhile.
    Wait(Duration),
    /// Give-up point reached.
    GiveUp,
}

#[derive(Debug, Clone)]
pub struct ResendTimer {
    start: Ticks,
    interval: Duration,
    give_up: Duration,
    /// Elapsed time of the last `Send`.
    last_send: Option<Duration>,
}

impl ResendTimer {
    pub fn new(start: Ticks, interval: Duration, give_up: Duration) -> Self {
        Self {
            start,
            interval,
            give_up,
            last_send: None,
        }
    }

    /// First call sends; then one send `interval` after the last one (no catch-up burst).
    pub fn poll(&mut self, now: Ticks) -> ResendAction {
        let elapsed = now.duration_since(self.start);
        if elapsed >= self.give_up {
            return ResendAction::GiveUp;
        }
        let next = match self.last_send {
            None => elapsed,
            Some(last) => last + self.interval,
        };
        if elapsed >= next {
            self.last_send = Some(elapsed);
            ResendAction::Send
        } else {
            ResendAction::Wait(next.min(self.give_up) - elapsed)
        }
    }
}

#[cfg(test)]
#[path = "resend_tests.rs"]
mod tests;
