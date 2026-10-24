//! Monotonic tick clock. On Windows the ticks are raw QPC, the same clock the `vdisplay` driver
//! writes into frame headers, so driver, service and tooling stamps compare directly.
//! Ticks from different machines are never comparable.

use std::time::Duration;

use crate::platform;

/// Raw monotonic ticks; `ticks_per_sec()` converts them to time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Ticks(u64);

impl Ticks {
    pub const ZERO: Ticks = Ticks(0);

    pub const fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    pub const fn raw(self) -> u64 {
        self.0
    }

    /// Time from `earlier` to `self`; `Duration::ZERO` if `earlier` is later.
    pub fn duration_since(self, earlier: Ticks) -> Duration {
        let delta = u128::from(self.0.saturating_sub(earlier.0));
        let nanos = delta * 1_000_000_000 / u128::from(ticks_per_sec());
        Duration::from_nanos(u64::try_from(nanos).unwrap_or(u64::MAX))
    }
}

// HOT PATH
#[inline]
pub fn now() -> Ticks {
    Ticks(platform::now_raw())
}

/// Tick frequency, read once per process; always >= 1.
pub fn ticks_per_sec() -> u64 {
    platform::ticks_per_sec()
}

#[cfg(test)]
#[path = "clock_tests.rs"]
mod tests;
