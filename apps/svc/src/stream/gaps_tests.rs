use mixlink_core::clock::ticks_per_sec;

use super::*;

fn ms(v: u64) -> Ticks {
    Ticks::from_raw(1_000 + v * ticks_per_sec() / 1000)
}

/// Feeds `n` frames 16 ms apart from `start`; returns the next time.
fn steady(g: &mut GapStats, start: u64, n: u64) -> u64 {
    for i in 0..n {
        g.frame(ms(start + i * 16));
    }
    start + n * 16
}

#[test]
fn still_screen_is_not_a_stall() {
    let mut g = GapStats::default();
    g.frame(ms(0));
    g.frame(ms(3000));
    g.frame(ms(3016));
    assert_eq!(g.stall_count(), 0);
    assert_eq!(g.longest(), Duration::from_millis(3000));
}

#[test]
fn gap_after_steady_run_is_a_stall() {
    let mut g = GapStats::default();
    let t = steady(&mut g, 0, 60);
    g.frame(ms(t + 600));
    assert_eq!(g.stall_count(), 1);
    assert_eq!(
        g.stalls(),
        &[Stall {
            at_ms: t - 16,
            gap_ms: 616
        }]
    );
}

#[test]
fn short_run_then_gap_is_not_a_stall() {
    let mut g = GapStats::default();
    let t = steady(&mut g, 0, 10);
    g.frame(ms(t + 600));
    assert_eq!(g.stall_count(), 0);
}

#[test]
fn keeps_sixteen_counts_all() {
    let mut g = GapStats::default();
    let mut t = 0;
    for _ in 0..20 {
        t = steady(&mut g, t, 40);
        t += 500;
    }
    assert_eq!(g.stall_count(), 19, "a stall between each two runs");
    assert_eq!(g.stalls().len(), 16);
}
