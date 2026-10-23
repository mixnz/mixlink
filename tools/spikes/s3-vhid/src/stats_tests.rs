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
fn merge_takes_sent_and_recv_halves() {
    let mut a = Samples::with_capacity(2);
    let mut b = Samples::with_capacity(2);
    a.record_sent(0, 10);
    a.record_sent(1, 20);
    b.record_recv(1, 25);
    b.record_sent(0, 999);
    let sum = summary(&Samples::merge(a, b), 1_000_000);
    assert_eq!((sum.sent, sum.received, sum.max_us), (2, 1, 5.0));
}

#[test]
fn recv_before_send_is_not_a_sample() {
    let mut s = Samples::with_capacity(1);
    s.record_recv(0, 6);
    let sum = summary(&s, 1_000_000);
    assert_eq!(sum.sent, 0);
    assert_eq!(sum.received, 0);
}

fn sender(times: &[i64]) -> Samples {
    let mut s = Samples::with_capacity(times.len());
    for (i, t) in times.iter().enumerate() {
        s.record_sent(i, *t);
    }
    s
}

#[test]
fn arrivals_in_order_are_all_assigned() {
    let s = assign_arrivals(sender(&[10, 20, 30]), 24, &[(0, 15), (1, 25), (2, 35)]);
    let sum = summary(&s, 1_000_000);
    assert_eq!((sum.received, sum.lost, sum.max_us), (3, 0, 5.0));
}

#[test]
fn stale_reading_before_any_send_is_dropped() {
    // A polled receiver's first reading is left over from an earlier run (residue 29412).
    let arrivals = [(29_412, 5), (0, 15), (1, 25)];
    let s = assign_arrivals(sender(&[10, 20]), 32_768, &arrivals);
    let sum = summary(&s, 1_000_000);
    assert_eq!((sum.received, sum.lost, sum.max_us), (2, 0, 5.0));
}

#[test]
fn a_lost_event_does_not_shift_the_others() {
    // Event 1 never arrives; 2 and 3 must keep their own send times.
    let s = assign_arrivals(sender(&[10, 20, 30, 40]), 24, &[(0, 11), (2, 32), (3, 43)]);
    let sum = summary(&s, 1_000_000);
    assert_eq!((sum.received, sum.lost), (3, 1));
    assert_eq!(sum.max_us, 3.0, "latencies 1, 2, 3 us");
}

#[test]
fn duplicate_or_late_residue_is_dropped() {
    let s = assign_arrivals(sender(&[10, 20]), 24, &[(0, 11), (0, 12), (1, 21), (0, 22)]);
    let sum = summary(&s, 1_000_000);
    assert_eq!((sum.received, sum.lost), (2, 0));
}

#[test]
fn residues_wrap_over_the_period() {
    let times: Vec<i64> = (0..60).map(|i| i * 10).collect();
    let arrivals: Vec<(u32, i64)> = (0..60u32)
        .map(|i| (i % 24, i64::from(i) * 10 + 3))
        .collect();
    let sum = summary(&assign_arrivals(sender(&times), 24, &arrivals), 1_000_000);
    assert_eq!((sum.received, sum.lost, sum.max_us), (60, 0, 3.0));
}

#[test]
fn arrival_is_matched_to_the_latest_send_with_its_residue() {
    // Residue 0 arrives after events 0..=24 were sent: it belongs to event 24, not 0.
    let times: Vec<i64> = (0..25).map(|i| i * 10).collect();
    let s = assign_arrivals(sender(&times), 24, &[(0, 245)]);
    let sum = summary(&s, 1_000_000);
    assert_eq!((sum.received, sum.max_us), (1, 5.0));
}

#[test]
fn pass_needs_p99_within_1_ms_and_no_loss_when_required() {
    let ok = Summary {
        p50_us: 100.0,
        p99_us: 900.0,
        max_us: 5000.0,
        sent: 10,
        received: 10,
        lost: 0,
    };
    assert!(passes(&ok, Loss::Forbidden));
    assert!(
        !passes(
            &Summary {
                received: 0,
                lost: 10,
                p99_us: 0.0,
                ..ok
            },
            Loss::Allowed
        ),
        "nothing received never passes"
    );
    let one_lost = Summary {
        received: 9,
        lost: 1,
        ..ok
    };
    assert!(
        !passes(&one_lost, Loss::Forbidden),
        "HID path: every event must arrive"
    );
    assert!(
        passes(&one_lost, Loss::Allowed),
        "polled state API may skip a state"
    );
    assert!(!passes(
        &Summary {
            p99_us: 1000.5,
            ..ok
        },
        Loss::Allowed
    ));
}

#[test]
fn arrivals_never_grow_past_their_capacity() {
    let mut a = Arrivals::with_capacity(2);
    a.push(1, 10);
    a.push(2, 20);
    a.push(3, 30);
    assert_eq!(a.as_slice(), &[(1, 10), (2, 20)]);
    assert_eq!(a.dropped(), 1);
}
