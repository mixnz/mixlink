use super::*;

#[test]
fn p99_of_known_samples() {
    let mut s = Samples::with_capacity(100);
    for v in (1..=100).rev() {
        s.push(v);
    }
    assert_eq!(s.percentile(50.0), 50);
    assert_eq!(s.percentile(99.0), 99);
    assert_eq!(s.max(), 100);
    assert_eq!(s.len(), 100);
}

#[test]
fn empty_samples_report_zero() {
    let s = Samples::with_capacity(4);
    assert_eq!(s.percentile(99.0), 0);
    assert_eq!(s.max(), 0);
}

#[test]
fn full_samples_count_overflow_without_growing() {
    let mut s = Samples::with_capacity(2);
    for v in [5, 6, 7] {
        s.push(v);
    }
    assert_eq!(s.len(), 2);
    assert_eq!(s.overflow(), 1);
}

#[test]
fn capacity_covers_the_run() {
    // 10 s at 120 Hz + 10 %.
    assert_eq!(expected_frames(10, 120_000), 1320);
    assert_eq!(expected_frames(1, 0), 66); // refresh unknown: 60 Hz
}
