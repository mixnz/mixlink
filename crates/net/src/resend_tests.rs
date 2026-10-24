use std::time::Duration;

use mixlink_core::clock::{Ticks, ticks_per_sec};

use super::*;

const BASE: u64 = 1_000_000;

fn at(ms: u64) -> Ticks {
    Ticks::from_raw(BASE + ms * ticks_per_sec() / 1000)
}

fn hello_timer() -> ResendTimer {
    ResendTimer::new(at(0), HELLO_RESEND, HELLO_GIVE_UP)
}

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

#[test]
fn hello_values() {
    assert_eq!(HELLO_RESEND, ms(250));
    assert_eq!(HELLO_GIVE_UP, Duration::from_secs(10));
}

#[test]
fn first_poll_sends() {
    assert_eq!(hello_timer().poll(at(0)), ResendAction::Send);
}

#[test]
fn sends_every_250_ms() {
    let mut timer = hello_timer();
    let got: Vec<ResendAction> = [0, 1, 249, 250, 499, 500]
        .into_iter()
        .map(|t| timer.poll(at(t)))
        .collect();
    assert_eq!(
        got,
        [
            ResendAction::Send,
            ResendAction::Wait(ms(249)),
            ResendAction::Wait(ms(1)),
            ResendAction::Send,
            ResendAction::Wait(ms(1)),
            ResendAction::Send,
        ]
    );
}

#[test]
fn no_catch_up_burst() {
    let mut timer = hello_timer();
    assert_eq!(timer.poll(at(0)), ResendAction::Send);
    assert_eq!(timer.poll(at(2000)), ResendAction::Send);
    assert_eq!(timer.poll(at(2000)), ResendAction::Wait(ms(250)));
}

#[test]
fn gives_up_at_10_s() {
    let mut timer = hello_timer();
    assert_eq!(timer.poll(at(0)), ResendAction::Send);
    assert_ne!(timer.poll(at(9999)), ResendAction::GiveUp);
    assert_eq!(timer.poll(at(10_000)), ResendAction::GiveUp);
}

#[test]
fn wait_stops_at_give_up() {
    let mut timer = ResendTimer::new(at(0), ms(250), ms(300));
    assert_eq!(timer.poll(at(0)), ResendAction::Send);
    assert_eq!(timer.poll(at(250)), ResendAction::Send);
    assert_eq!(timer.poll(at(260)), ResendAction::Wait(ms(40)));
}

#[test]
fn huge_interval_waits_until_give_up() {
    let mut timer = ResendTimer::new(at(0), Duration::MAX, ms(10_000));
    assert_eq!(timer.poll(at(1)), ResendAction::Send);
    assert_eq!(timer.poll(at(2)), ResendAction::Wait(ms(9998)));
}
