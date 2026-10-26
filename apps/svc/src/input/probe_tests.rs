use mixlink_input::hid::keyboard::KeyboardState;
use mixlink_input::hid::report::Target;

use super::*;

fn latencies(sent: &[i64], period: u32, arrivals: &[(u32, i64)], freq: i64) -> Summary {
    summary(sent, &assign_arrivals(sent, period, arrivals), freq)
}

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
fn summary_in_microseconds() {
    // 10 MHz QPC: 10 ticks = 1 us.
    let s = summary(&[0, 100], &[Some(10), Some(130)], 10_000_000);
    assert_eq!(
        (s.p50_us, s.p99_us, s.max_us, s.sent, s.received, s.lost),
        (1.0, 3.0, 3.0, 2, 2, 0)
    );
}

#[test]
fn lost_counts_missing() {
    let s = summary(&[1, 2, 3, 4], &[Some(2), None, Some(5), None], 1_000_000);
    assert_eq!((s.sent, s.received, s.lost), (4, 2, 2));
}

#[test]
fn arrivals_in_order_are_all_assigned() {
    let s = latencies(&[10, 20, 30], 24, &[(0, 15), (1, 25), (2, 35)], 1_000_000);
    assert_eq!((s.received, s.lost, s.max_us), (3, 0, 5.0));
}

#[test]
fn stale_reading_before_any_send_is_dropped() {
    let s = latencies(
        &[10, 20],
        32_768,
        &[(29_412, 5), (0, 15), (1, 25)],
        1_000_000,
    );
    assert_eq!((s.received, s.lost, s.max_us), (2, 0, 5.0));
}

#[test]
fn a_lost_event_does_not_shift_the_others() {
    let s = latencies(
        &[10, 20, 30, 40],
        24,
        &[(0, 11), (2, 32), (3, 43)],
        1_000_000,
    );
    assert_eq!((s.received, s.lost, s.max_us), (3, 1, 3.0));
}

#[test]
fn duplicate_or_late_residue_is_dropped() {
    let s = latencies(
        &[10, 20],
        24,
        &[(0, 11), (0, 12), (1, 21), (0, 22)],
        1_000_000,
    );
    assert_eq!((s.received, s.lost), (2, 0));
}

#[test]
fn residues_wrap_over_the_period() {
    let sent: Vec<i64> = (0..60).map(|i| i * 10).collect();
    let arrivals: Vec<(u32, i64)> = (0..60u32)
        .map(|i| (i % 24, i64::from(i) * 10 + 3))
        .collect();
    let s = latencies(&sent, 24, &arrivals, 1_000_000);
    assert_eq!((s.received, s.lost, s.max_us), (60, 0, 3.0));
}

#[test]
fn arrival_is_matched_to_the_latest_send_with_its_residue() {
    let sent: Vec<i64> = (0..25).map(|i| i * 10).collect();
    let s = latencies(&sent, 24, &[(0, 245)], 1_000_000);
    assert_eq!((s.received, s.max_us), (1, 5.0));
}

#[test]
fn decode_roundtrip_all_devices() {
    for i in 0..3000u32 {
        let (usage, down) = encode_key(i);
        let vkey = u16::from(usage - USAGE_F13) + VK_F13;
        assert_eq!(decode_key(vkey, !down), Some(i % KEY_PERIOD), "key {i}");
        assert_eq!(
            decode_rel(i32::from(encode_rel(i))),
            Some(i % REL_PERIOD),
            "rel {i}"
        );
        let normalized = (f64::from(encode_abs(i)) * 65535.0 / 32767.0).round() as i32;
        assert_eq!(decode_abs(normalized), Some(i % ABS_PERIOD), "abs {i}");
        let lx = PadState::NEUTRAL;
        let lx = PadState {
            lx: encode_pad_lx(i),
            ..lx
        }
        .report(0);
        let raw = u16::from_le_bytes([lx.payload()[0], lx.payload()[1]]);
        assert_eq!(decode_pad_lx(raw), Some(i % PAD_PERIOD), "pad {i}");
    }
}

#[test]
fn decode_rejects_values_outside_the_code() {
    assert_eq!(decode_key(VK_F13 - 1, false), None);
    assert_eq!(decode_key(VK_F13 + 12, false), None);
    assert_eq!(decode_rel(0), None);
    assert_eq!(decode_rel(101), None);
    assert_eq!(decode_abs(0), None);
    assert_eq!(
        decode_pad_lx(0x8000),
        None,
        "a centred stick is never a code"
    );
}

#[test]
fn report_for_matches_the_device() {
    let mut keyboard = KeyboardState::default();
    let cases = [
        (ProbeDevice::Kbd, Target::Keyboard),
        (ProbeDevice::Rel, Target::MouseRel),
        (ProbeDevice::Abs, Target::MouseAbs),
        (ProbeDevice::Pad, Target::Pad(0)),
    ];
    for (device, target) in cases {
        assert_eq!(report_for(device, 5, &mut keyboard, 0).target(), target);
    }
    // Key events alternate press / release, so the keyboard ends empty after an even count.
    let mut keyboard = KeyboardState::default();
    for i in 0..48 {
        report_for(ProbeDevice::Kbd, i, &mut keyboard, 0);
    }
    assert!(keyboard.report().payload().iter().all(|&b| b == 0));
}

#[test]
fn pad_probe_reports_go_to_the_slot() {
    let mut keyboard = KeyboardState::default();
    assert_eq!(
        report_for(ProbeDevice::Pad, 1, &mut keyboard, 3).target(),
        Target::Pad(3)
    );
}

#[test]
fn xinput_lx_roundtrip() {
    for i in [0u32, 1, 12_345, 32_767, 32_768, 70_000] {
        let raw = i % PAD_PERIOD;
        // xinputhid maps the HID axis 0..=65535 onto -32768..=32767.
        let thumb = i16::try_from(i64::from(raw) - 32_768).unwrap();
        assert_eq!(decode_xinput_lx(thumb), Some(raw), "{i}");
    }
    assert_eq!(decode_xinput_lx(0), None, "a centred stick is never a code");
}
