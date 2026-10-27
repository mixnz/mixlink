//! Caps broadcast replies per source address, so a probe flood cannot turn the host into an amplifier.

use std::collections::HashMap;
use std::net::IpAddr;
use std::time::{Duration, Instant};

pub const REPLY_WINDOW: Duration = Duration::from_millis(250);
/// Above this many remembered sources, entries older than the window are dropped.
const PRUNE_ABOVE: usize = 256;

#[derive(Debug, Default)]
pub struct ReplyLimiter {
    last: HashMap<IpAddr, Instant>,
}

impl ReplyLimiter {
    pub fn new() -> ReplyLimiter {
        ReplyLimiter::default()
    }

    /// At most one `true` per source per `REPLY_WINDOW`.
    pub fn allow(&mut self, src: IpAddr, now: Instant) -> bool {
        if let Some(last) = self.last.get(&src)
            && now.saturating_duration_since(*last) < REPLY_WINDOW
        {
            return false;
        }
        self.last.insert(src, now);
        if self.last.len() > PRUNE_ABOVE {
            self.last
                .retain(|_, at| now.saturating_duration_since(*at) < REPLY_WINDOW);
        }
        true
    }

    pub fn len(&self) -> usize {
        self.last.len()
    }

    pub fn is_empty(&self) -> bool {
        self.last.is_empty()
    }
}

#[cfg(test)]
#[path = "limiter_tests.rs"]
mod tests;
