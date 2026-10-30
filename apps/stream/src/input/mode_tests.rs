use std::time::{Duration, Instant};

use super::*;

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

#[test]
fn absolute_until_hidden_for_the_hold() {
    let t0 = Instant::now();
    let mut p = ModePicker::default();
    assert_eq!(p.mode(), MouseMode::Absolute);
    p.host_cursor(false, t0);
    assert_eq!(p.update(t0 + ms(149)), MouseMode::Absolute);
    assert_eq!(p.next_check(), Some(t0 + HIDE_HOLD));
    assert_eq!(p.update(t0 + ms(150)), MouseMode::Relative);
    assert_eq!(p.next_check(), None);
}

#[test]
fn shown_again_within_the_hold_stays_absolute() {
    let t0 = Instant::now();
    let mut p = ModePicker::default();
    p.host_cursor(false, t0);
    p.host_cursor(true, t0 + ms(50));
    assert_eq!(p.update(t0 + ms(500)), MouseMode::Absolute);
    assert_eq!(p.next_check(), None);
}

#[test]
fn held_button_keeps_absolute_while_dragging() {
    let t0 = Instant::now();
    let mut p = ModePicker::default();
    p.button(0, true, t0);
    p.host_cursor(false, t0 + ms(10));
    assert_eq!(p.update(t0 + ms(1000)), MouseMode::Absolute);
    assert_eq!(p.next_check(), None);
    // Released while still hidden: the hold starts again from the release.
    p.button(0, false, t0 + ms(1000));
    assert_eq!(p.update(t0 + ms(1100)), MouseMode::Absolute);
    p.host_cursor(true, t0 + ms(1120));
    assert_eq!(p.update(t0 + ms(2000)), MouseMode::Absolute);
}

#[test]
fn relative_stays_while_shooting() {
    let t0 = Instant::now();
    let mut p = ModePicker::default();
    p.host_cursor(false, t0);
    assert_eq!(p.update(t0 + ms(200)), MouseMode::Relative);
    p.button(0, true, t0 + ms(300));
    assert_eq!(p.update(t0 + ms(400)), MouseMode::Relative);
    p.host_cursor(true, t0 + ms(500));
    assert_eq!(p.update(t0 + ms(500)), MouseMode::Absolute);
}
