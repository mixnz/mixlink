use super::*;

#[test]
fn percentiles_nearest_rank() {
    let mut samples: Vec<u64> = (1..=100).rev().collect();
    let s = percentiles(&mut samples);
    assert_eq!(
        s,
        Stats {
            p50: 50,
            p99: 99,
            max: 100,
            count: 100
        }
    );
}

#[test]
fn percentiles_single_sample() {
    let s = percentiles(&mut [7]);
    assert_eq!((s.p50, s.p99, s.max, s.count), (7, 7, 7, 1));
}

#[test]
fn percentiles_empty_is_zero() {
    assert_eq!(percentiles(&mut []), Stats::default());
}
