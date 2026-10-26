use std::time::Duration;

use mixlink_input::hid::gamepad::PadState;
use mixlink_input::hid::report::{Report, Target};
use mixlink_input::platform::windows::vhid::{ReportSink, VHidError};

use super::*;

/// Records reports; fails every feed with `InUse` when `in_use`.
#[derive(Default)]
struct FakeSink {
    reports: Vec<Report>,
    in_use: bool,
}

impl ReportSink for FakeSink {
    fn feed(&mut self, report: &Report) -> Result<(), VHidError> {
        if self.in_use {
            return Err(VHidError::InUse);
        }
        self.reports.push(*report);
        Ok(())
    }
}

fn no_wait(_: Duration) -> bool {
    false
}

fn keys(steps: &[Step]) -> Vec<(u8, bool)> {
    steps
        .iter()
        .filter_map(|s| match s.action {
            Action::Key(usage, down) => Some((usage, down)),
            Action::Rel(..) | Action::Abs(..) | Action::Pad(_) => None,
        })
        .collect()
}

fn played(script: Script) -> Vec<Report> {
    let mut sink = FakeSink::default();
    play(&steps(script), &mut sink, &mut no_wait).unwrap();
    sink.reports
}

#[test]
fn keys_script_side_effect_free() {
    let all = keys(&steps(Script::Keys));
    assert!(!all.is_empty());
    assert!(all.iter().all(|&(usage, _)| (0x68..=0x73).contains(&usage)));
    for usage in 0x68..=0x73u8 {
        assert_eq!(all.iter().filter(|k| **k == (usage, true)).count(), 1);
        assert_eq!(all.iter().filter(|k| **k == (usage, false)).count(), 1);
    }
    let last = played(Script::Keys).pop().unwrap();
    assert!(last.payload().iter().all(|&b| b == 0));
}

#[test]
fn chord_holds_ten_keys() {
    let reports = played(Script::Chord);
    let most = reports
        .iter()
        .map(|r| r.payload().iter().map(|b| b.count_ones()).sum::<u32>())
        .max()
        .unwrap();
    assert_eq!(most, 10);
    assert!(reports.last().unwrap().payload().iter().all(|&b| b == 0));
}

#[test]
fn mouse_script_returns_home() {
    let (mut x, mut y) = (0i32, 0i32);
    for step in steps(Script::Mouse) {
        if let Action::Rel(dx, dy) = step.action {
            x += dx;
            y += dy;
        }
    }
    assert_eq!((x, y), (0, 0));
}

#[test]
fn pad_script_ends_neutral() {
    let reports = played(Script::Pad);
    assert!(reports.len() > 10);
    assert!(reports.iter().all(|r| r.target() == Target::Gamepad));
    assert_eq!(*reports.last().unwrap(), PadState::NEUTRAL.report());
}

#[test]
fn all_is_concatenation() {
    let count = |script| steps(script).len();
    assert_eq!(
        count(Script::All),
        count(Script::Keys) + count(Script::Mouse) + count(Script::Abs) + count(Script::Pad)
    );
}

#[test]
fn steps_sorted_by_time() {
    for script in [
        Script::Keys,
        Script::Chord,
        Script::Mouse,
        Script::Abs,
        Script::Pad,
        Script::All,
        Script::Hold,
    ] {
        let all = steps(script);
        assert!(
            all.windows(2).all(|w| w[0].at_ms <= w[1].at_ms),
            "{script:?}"
        );
    }
}

#[test]
fn hold_presses_and_never_releases() {
    assert_eq!(keys(&steps(Script::Hold)), [(0x68, true)]);
}

#[test]
fn try_reports_held_feeder() {
    let mut sink = FakeSink {
        in_use: true,
        ..FakeSink::default()
    };
    let result = play(&steps(Script::Keys), &mut sink, &mut no_wait);
    assert!(matches!(result, Err(VHidError::InUse)));
    assert!(sink.reports.is_empty());
}

#[test]
fn play_stops_when_wait_says_so() {
    let mut sink = FakeSink::default();
    let mut stop_now = |_: Duration| true;
    play(&steps(Script::Keys), &mut sink, &mut stop_now).unwrap();
    assert_eq!(sink.reports.len(), 1);
}
