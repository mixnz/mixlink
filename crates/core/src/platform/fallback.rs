//! Non-Windows clock (tests and tooling only): nanoseconds since a process-wide anchor.

use std::sync::OnceLock;
use std::time::Instant;

fn anchor() -> Instant {
    static ANCHOR: OnceLock<Instant> = OnceLock::new();
    *ANCHOR.get_or_init(Instant::now)
}

// HOT PATH
#[inline]
pub(crate) fn now_raw() -> u64 {
    u64::try_from(anchor().elapsed().as_nanos()).unwrap_or(u64::MAX)
}

pub(crate) fn ticks_per_sec() -> u64 {
    1_000_000_000
}
