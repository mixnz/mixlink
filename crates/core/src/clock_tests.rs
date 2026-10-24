use std::time::Duration;
use std::time::Instant;

use super::*;

#[test]
fn now_is_monotonic() {
    let mut last = now();
    for _ in 0..1_000_000 {
        let t = now();
        assert!(t >= last);
        last = t;
    }
}

#[test]
fn ticks_per_sec_is_positive() {
    assert!(ticks_per_sec() > 0);
}

#[test]
fn duration_since_uses_frequency() {
    let f = ticks_per_sec();
    assert_eq!(
        Ticks::from_raw(f).duration_since(Ticks::ZERO),
        Duration::from_secs(1)
    );
    let half = Ticks::from_raw(f / 2).duration_since(Ticks::ZERO);
    let diff = half.abs_diff(Duration::from_millis(500));
    assert!(
        diff <= Duration::from_nanos(1),
        "half second off by {diff:?}"
    );
}

#[test]
fn duration_since_saturates() {
    let earlier = Ticks::from_raw(10);
    let later = Ticks::from_raw(20);
    assert_eq!(earlier.duration_since(later), Duration::ZERO);
}

#[test]
fn now_cost_is_reported() {
    const CALLS: u32 = 1_000_000;
    let start = Instant::now();
    let mut last = Ticks::ZERO;
    for _ in 0..CALLS {
        last = now();
    }
    let per_call = start.elapsed() / CALLS;
    eprintln!("clock::now: {per_call:?} per call (last {})", last.raw());
}
