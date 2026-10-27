use super::*;

#[test]
fn median_of_samples() {
    let mut rtt = Rtt::default();
    assert_eq!(rtt.median_us(), 0);
    let t0 = Instant::now();
    for (n, ms) in [(0u32, 3u64), (1, 1), (2, 2)] {
        rtt.sent(n, t0);
        rtt.pong(n, t0 + Duration::from_millis(ms));
    }
    assert_eq!(rtt.median_us(), 2000);
    assert_eq!(rtt.one_way(), Duration::from_millis(1));
}

#[test]
fn unknown_or_repeated_pong_ignored() {
    let mut rtt = Rtt::default();
    let t0 = Instant::now();
    rtt.pong(5, t0);
    assert_eq!(rtt.median_us(), 0);
    rtt.sent(1, t0);
    rtt.pong(1, t0 + Duration::from_millis(4));
    rtt.pong(1, t0 + Duration::from_millis(400));
    assert_eq!(rtt.median_us(), 4000);
    // Ping 9 reuses ping 1's slot: a late pong 1 is not counted.
    rtt.sent(9, t0);
    rtt.pong(1, t0 + Duration::from_millis(900));
    assert_eq!(rtt.median_us(), 4000);
}

#[test]
fn window_keeps_last_sixteen() {
    let mut rtt = Rtt::default();
    let t0 = Instant::now();
    for n in 0..40u32 {
        let ms = if n < 24 { 50 } else { 1 };
        rtt.sent(n, t0);
        rtt.pong(n, t0 + Duration::from_millis(ms));
    }
    assert_eq!(rtt.median_us(), 1000);
}
