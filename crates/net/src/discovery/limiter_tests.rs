use std::net::{IpAddr, Ipv4Addr};
use std::time::{Duration, Instant};

use super::*;

fn ip(last: u8) -> IpAddr {
    IpAddr::V4(Ipv4Addr::new(192, 168, 1, last))
}

#[test]
fn limiter_one_reply_per_window() {
    let t0 = Instant::now();
    let mut limiter = ReplyLimiter::new();
    assert!(limiter.allow(ip(1), t0));
    assert!(!limiter.allow(ip(1), t0 + Duration::from_millis(100)));
    assert!(limiter.allow(ip(2), t0 + Duration::from_millis(100)));
    assert!(limiter.allow(ip(1), t0 + REPLY_WINDOW));
}

#[test]
fn limiter_forgets_old_sources() {
    let t0 = Instant::now();
    let mut limiter = ReplyLimiter::new();
    for last in 0..=255 {
        limiter.allow(ip(last), t0);
    }
    limiter.allow(
        IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)),
        t0 + Duration::from_secs(1),
    );
    assert!(limiter.len() <= 2);
}
