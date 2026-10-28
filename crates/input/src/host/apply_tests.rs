use mixlink_core::input::{BUTTON_CODE, InputEvent, MAX_EVENTS, MouseMode};

use super::*;
use crate::client::state::InputState;
use crate::hid::keyboard::KeyboardState;
use crate::hid::report::{Report, Target};

#[derive(Default)]
struct FakeSink {
    reports: Vec<Report>,
}

impl ReportSink for FakeSink {
    type Error = ();

    fn feed(&mut self, report: &Report) -> Result<(), ()> {
        self.reports.push(*report);
        Ok(())
    }
}

fn applier() -> InputApplier {
    InputApplier::new(AbsMap::new((1920, 1080), None))
}

fn keyboard(usages: &[u8]) -> Report {
    let mut k = KeyboardState::default();
    for &u in usages {
        k.set(u, true);
    }
    k.report()
}

fn rel(p: &Report) -> (u8, i16, i16, i8, i8) {
    let b = p.payload();
    (
        b[0],
        i16::from_le_bytes([b[1], b[2]]),
        i16::from_le_bytes([b[3], b[4]]),
        b[5] as i8,
        b[6] as i8,
    )
}

#[test]
fn stale_seq_feeds_nothing() {
    let (mut a, mut sink, mut s) = (applier(), FakeSink::default(), InputState::new());
    s.key(0x04, true);
    let p = s.packet();
    assert_eq!(a.apply(&p, &mut sink), Ok(Applied::Fresh));
    let n = sink.reports.len();
    assert_eq!(a.apply(&p, &mut sink), Ok(Applied::Stale));
    assert_eq!(sink.reports.len(), n);
    assert_eq!(a.counts().stale, 1);
}

#[test]
fn key_tap_lost_then_recovered() {
    let (mut a, mut sink, mut s) = (applier(), FakeSink::default(), InputState::new());
    let first = s.packet();
    a.apply(&first, &mut sink).unwrap();
    s.key(0x04, true);
    let _lost = s.packet();
    s.key(0x04, false);
    let p = s.packet();
    a.apply(&p, &mut sink).unwrap();
    assert_eq!(sink.reports, vec![keyboard(&[0x04]), keyboard(&[])]);
    assert_eq!(a.counts().recovered_events, 1);
    assert!(!a.held());
}

#[test]
fn gap_beyond_ring_reconciles_bitmap() {
    let (mut a, mut sink, mut s) = (applier(), FakeSink::default(), InputState::new());
    a.apply(&s.packet(), &mut sink).unwrap();
    s.key(0x2C, true);
    let _lost = s.packet();
    for usage in 0x04..0x04 + MAX_EVENTS as u8 {
        s.key(usage, true);
        s.key(usage, false);
    }
    a.apply(&s.packet(), &mut sink).unwrap();
    assert_eq!(sink.reports.last(), Some(&keyboard(&[0x2C])));
    assert_eq!(a.counts().bitmap_fixes, 1);
    assert!(a.held());
}

#[test]
fn totals_wrap() {
    let (mut a, mut sink, mut s) = (applier(), FakeSink::default(), InputState::new());
    s.set_mode(MouseMode::Relative);
    s.mouse_rel(i32::MAX - 1, 0);
    a.apply(&s.packet(), &mut sink).unwrap();
    assert!(sink.reports.is_empty());
    s.mouse_rel(3, 0);
    a.apply(&s.packet(), &mut sink).unwrap();
    assert_eq!(sink.reports.len(), 1);
    assert_eq!(rel(&sink.reports[0]), (0, 3, 0, 0, 0));
}

#[test]
fn first_packet_sets_baseline() {
    let (mut a, mut sink, mut s) = (applier(), FakeSink::default(), InputState::new());
    s.set_mode(MouseMode::Relative);
    s.mouse_rel(500, 500);
    s.wheel(240, 0);
    a.apply(&s.packet(), &mut sink).unwrap();
    assert!(sink.reports.is_empty());
}

#[test]
fn wheel_remainder_carries() {
    let (mut a, mut sink, mut s) = (applier(), FakeSink::default(), InputState::new());
    s.set_mode(MouseMode::Relative);
    a.apply(&s.packet(), &mut sink).unwrap();
    s.wheel(60, 0);
    a.apply(&s.packet(), &mut sink).unwrap();
    assert!(sink.reports.is_empty());
    s.wheel(60, -120);
    a.apply(&s.packet(), &mut sink).unwrap();
    assert_eq!(rel(&sink.reports[0]), (0, 0, 0, 1, -1));
}

#[test]
fn absolute_moves_through_map() {
    let (mut a, mut sink, mut s) = (applier(), FakeSink::default(), InputState::new());
    s.set_mode(MouseMode::Absolute);
    s.mouse_abs(1919, 0);
    a.apply(&s.packet(), &mut sink).unwrap();
    assert_eq!(sink.reports.len(), 1);
    assert_eq!(sink.reports[0].target(), Target::MouseAbs);
    assert_eq!(&sink.reports[0].payload()[1..5], &[0xFF, 0x7F, 0, 0]);
    a.apply(&s.packet(), &mut sink).unwrap();
    assert_eq!(sink.reports.len(), 1);
}

#[test]
fn button_click_goes_to_relative_collection() {
    let (mut a, mut sink, mut s) = (applier(), FakeSink::default(), InputState::new());
    s.button(0, true);
    a.apply(&s.packet(), &mut sink).unwrap();
    assert_eq!(rel(&sink.reports[0]), (1, 0, 0, 0, 0));
    s.button(0, false);
    a.apply(&s.packet(), &mut sink).unwrap();
    assert_eq!(rel(&sink.reports[1]), (0, 0, 0, 0, 0));
}

#[test]
fn unknown_codes_are_ignored() {
    let (mut a, mut sink) = (applier(), FakeSink::default());
    let mut p = InputState::new().packet();
    p.count = 2;
    p.first_event = 1;
    p.events[0] = InputEvent {
        code: BUTTON_CODE + 7,
        down: true,
    };
    p.events[1] = InputEvent {
        code: 0x01,
        down: true,
    };
    a.apply(&p, &mut sink).unwrap();
    assert!(sink.reports.is_empty());
}

#[test]
fn release_all_neutral() {
    let (mut a, mut sink, mut s) = (applier(), FakeSink::default(), InputState::new());
    s.key(0x04, true);
    s.button(1, true);
    a.apply(&s.packet(), &mut sink).unwrap();
    assert!(a.held());
    sink.reports.clear();
    a.release_all(&mut sink).unwrap();
    assert_eq!(sink.reports[0], keyboard(&[]));
    assert_eq!(rel(&sink.reports[1]), (0, 0, 0, 0, 0));
    assert!(!a.held());
}
