use super::keyboard::KeyboardState;
use super::mouse::{REL_MAX, mouse_abs_report, mouse_rel_reports};
use super::report::{PAYLOAD_MAX, Report, Target, neutral_reports};
use super::xbox::{Dpad, PAD_SLOTS, PadState, buttons};

fn bit(report: &Report, byte: usize, bit: u8) -> bool {
    report.payload()[byte] & (1 << bit) != 0
}

fn rel_xy(report: &Report) -> (i32, i32) {
    let p = report.payload();
    (
        i32::from(i16::from_le_bytes([p[1], p[2]])),
        i32::from(i16::from_le_bytes([p[3], p[4]])),
    )
}

#[test]
fn payload_lengths() {
    assert_eq!(Target::Keyboard.payload_len(), 16);
    assert_eq!(Target::MouseRel.payload_len(), 7);
    assert_eq!(Target::MouseAbs.payload_len(), 5);
    assert_eq!(Target::Pad(0).payload_len(), 16);
    assert_eq!(PAYLOAD_MAX, 16);
}

#[test]
fn keyboard_modifier_and_bitmap() {
    let mut kb = KeyboardState::default();
    assert!(kb.set(0xE1, true));
    assert!(kb.set(0x04, true));
    assert!(kb.set(0x73, true));
    let r = kb.report();
    assert_eq!(r.target(), Target::Keyboard);
    assert_eq!(r.payload().len(), 16);
    assert_eq!(r.payload()[0], 0x02);
    assert!(bit(&r, 1, 0));
    assert!(bit(&r, 14, 7));
}

#[test]
fn keyboard_international() {
    let mut kb = KeyboardState::default();
    assert!(kb.set(0x87, true));
    assert!(kb.set(0x8B, true));
    let r = kb.report();
    assert!(bit(&r, 15, 0));
    assert!(bit(&r, 15, 4));
    assert_eq!(r.payload()[15] & 0xE0, 0);
}

#[test]
fn keyboard_ignores_unknown_usage() {
    let mut kb = KeyboardState::default();
    assert!(kb.set(0x05, true));
    let before = kb.report();
    for usage in [0x00, 0x03, 0x74, 0x86, 0x8C, 0xE8, 0xFF] {
        assert!(!kb.set(usage, true), "usage {usage:#x}");
    }
    assert_eq!(kb.report(), before);
}

#[test]
fn keyboard_ten_keys_held() {
    let mut kb = KeyboardState::default();
    for usage in 0x04..=0x0D {
        assert!(kb.set(usage, true));
    }
    let set: u32 = kb.report().payload()[1..]
        .iter()
        .map(|b| b.count_ones())
        .sum();
    assert_eq!(set, 10);
}

#[test]
fn keyboard_release_clears() {
    let mut kb = KeyboardState::default();
    kb.set(0x04, true);
    kb.set(0xE0, true);
    kb.set(0x04, false);
    kb.set(0xE0, false);
    assert!(kb.report().payload().iter().all(|&b| b == 0));
    kb.set(0x10, true);
    kb.clear();
    assert!(kb.report().payload().iter().all(|&b| b == 0));
}

#[test]
fn rel_small_delta() {
    let reports = mouse_rel_reports(0, -5, 3, 1, 0);
    assert_eq!(reports.as_slice().len(), 1);
    assert_eq!(
        reports.as_slice()[0].payload(),
        &[0, 0xFB, 0xFF, 0x03, 0x00, 0x01, 0x00]
    );
}

#[test]
fn rel_splits_large_delta() {
    let reports = mouse_rel_reports(1, 40_000, 0, 2, 0);
    let all = reports.as_slice();
    assert_eq!(all.len(), 2);
    let sum: i32 = all.iter().map(|r| rel_xy(r).0).sum();
    assert_eq!(sum, 40_000);
    assert!(all.iter().all(|r| rel_xy(r).0 <= REL_MAX));
    assert_eq!(all[0].payload()[5], 2);
    assert_eq!(all[1].payload()[5], 0);
    assert!(all.iter().all(|r| r.payload()[0] == 1));
}

#[test]
fn rel_caps_at_four_reports() {
    let reports = mouse_rel_reports(0, 1_000_000, 0, 0, 0);
    let all = reports.as_slice();
    assert_eq!(all.len(), 4);
    assert!(all.iter().all(|r| rel_xy(r).0 == REL_MAX));
}

#[test]
fn rel_negative_min() {
    let reports = mouse_rel_reports(0, -40_000, 0, 0, 0);
    let xs: Vec<i32> = reports.as_slice().iter().map(|r| rel_xy(r).0).collect();
    assert_eq!(xs, [-32_767, -7_233]);
}

#[test]
fn rel_zero_still_sends_buttons() {
    let reports = mouse_rel_reports(0b1_0001, 0, 0, 0, 0);
    assert_eq!(reports.as_slice().len(), 1);
    assert_eq!(reports.as_slice()[0].payload()[0], 0b1_0001);
}

#[test]
fn abs_clamps() {
    let r = mouse_abs_report(0xFF, 40_000, 100);
    assert_eq!(r.target(), Target::MouseAbs);
    assert_eq!(r.payload(), &[0x1F, 0xFF, 0x7F, 100, 0]);
}

#[test]
fn wire_ids() {
    assert_eq!(Target::Keyboard.wire_id(), 1);
    assert_eq!(Target::MouseRel.wire_id(), 2);
    assert_eq!(Target::MouseAbs.wire_id(), 3);
    assert_eq!(Target::Pad(0).wire_id(), 0x10);
    assert_eq!(Target::Pad(3).wire_id(), 0x13);
}

#[test]
fn pad_neutral_bytes() {
    let r = PadState::NEUTRAL.report(2);
    assert_eq!(r.target(), Target::Pad(2));
    assert_eq!(
        r.payload(),
        &[0, 0x80, 0, 0x80, 0, 0x80, 0, 0x80, 0, 0, 0, 0, 0, 0, 0, 0]
    );
}

#[test]
fn pad_report_layout() {
    let pad = PadState {
        lx: -32_768,
        ly: 32_767,
        lt: 65_535,
        rt: 32_768,
        ..PadState::NEUTRAL
    };
    let r = pad.report(0);
    let p = r.payload();
    assert_eq!(&p[0..2], &[0x00, 0x00]);
    assert_eq!(&p[2..4], &[0xFF, 0xFF]);
    assert_eq!(u16::from_le_bytes([p[8], p[9]]), 1023);
    assert_eq!(u16::from_le_bytes([p[10], p[11]]), 512);
}

#[test]
fn pad_dpad_values() {
    let all = [
        (Dpad::Up, 1),
        (Dpad::UpRight, 2),
        (Dpad::Right, 3),
        (Dpad::DownRight, 4),
        (Dpad::Down, 5),
        (Dpad::DownLeft, 6),
        (Dpad::Left, 7),
        (Dpad::UpLeft, 8),
        (Dpad::Centered, 0),
    ];
    for (dpad, hat) in all {
        let pad = PadState {
            dpad,
            ..PadState::NEUTRAL
        };
        assert_eq!(pad.report(0).payload()[12], hat, "{dpad:?}");
    }
}

#[test]
fn pad_buttons_map_to_report_bits() {
    let bits = |buttons: u16| {
        let r = PadState {
            buttons,
            ..PadState::NEUTRAL
        }
        .report(0);
        u16::from_le_bytes([r.payload()[13], r.payload()[14]])
    };
    assert_eq!(bits(buttons::A | buttons::MENU), 0x0801);
    assert_eq!(bits(buttons::B), 1 << 1);
    assert_eq!(bits(buttons::X), 1 << 3);
    assert_eq!(bits(buttons::Y), 1 << 4);
    assert_eq!(bits(buttons::LB | buttons::RB), (1 << 6) | (1 << 7));
    assert_eq!(bits(buttons::VIEW), 1 << 10);
    assert_eq!(bits(buttons::GUIDE), 1 << 12);
    assert_eq!(bits(buttons::LS | buttons::RS), (1 << 13) | (1 << 14));
    let share = PadState {
        share: true,
        ..PadState::NEUTRAL
    }
    .report(0);
    assert_eq!(share.payload()[15], 1);
}

#[test]
fn neutral_reports_release_everything() {
    let all = neutral_reports();
    assert_eq!(all.len(), 2 + usize::from(PAD_SLOTS));
    assert_eq!(all[0].target(), Target::Keyboard);
    assert!(all[0].payload().iter().all(|&b| b == 0));
    assert_eq!(all[1].target(), Target::MouseRel);
    assert!(all[1].payload().iter().all(|&b| b == 0));
    for slot in 0..PAD_SLOTS {
        assert_eq!(all[2 + usize::from(slot)], PadState::NEUTRAL.report(slot));
    }
}
