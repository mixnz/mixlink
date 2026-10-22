use super::*;

#[test]
fn nearest_rank_p99() {
    let sorted: Vec<i64> = (1..=100).collect();
    assert_eq!(nearest_rank(&sorted, 99), Some(99));
    assert_eq!(nearest_rank(&sorted, 50), Some(50));
    assert_eq!(nearest_rank(&sorted, 100), Some(100));
    assert_eq!(nearest_rank(&[], 50), None);
    assert_eq!(nearest_rank(&[7], 99), Some(7));
}

#[test]
fn lost_counts_missing() {
    let mut s = Samples::with_capacity(4);
    for i in 0..4 {
        s.record_sent(i, 1_000 * (i as i64 + 1));
    }
    s.record_recv(0, 1_010);
    s.record_recv(2, 3_020);
    let sum = summary(&s, 1_000_000);
    assert_eq!(sum.sent, 4);
    assert_eq!(sum.received, 2);
    assert_eq!(sum.lost, 2);
}

#[test]
fn summary_in_microseconds() {
    let mut s = Samples::with_capacity(2);
    s.record_sent(0, 0);
    s.record_sent(1, 100);
    s.record_recv(0, 10);
    s.record_recv(1, 130);
    // 10 MHz QPC: 10 ticks = 1 us.
    let sum = summary(&s, 10_000_000);
    assert_eq!(sum.p50_us, 1.0);
    assert_eq!(sum.p99_us, 3.0);
    assert_eq!(sum.max_us, 3.0);
}

#[test]
fn recv_ignores_out_of_range_and_repeats() {
    let mut s = Samples::with_capacity(1);
    s.record_sent(0, 5);
    s.record_recv(0, 6);
    s.record_recv(0, 99);
    s.record_recv(7, 8);
    s.record_sent(9, 1);
    let sum = summary(&s, 1_000_000);
    assert_eq!(sum.received, 1);
    assert_eq!(sum.max_us, 1.0);
}

#[test]
fn recv_before_send_is_not_a_sample() {
    let mut s = Samples::with_capacity(1);
    s.record_recv(0, 6);
    let sum = summary(&s, 1_000_000);
    assert_eq!(sum.sent, 0);
    assert_eq!(sum.received, 0);
}
