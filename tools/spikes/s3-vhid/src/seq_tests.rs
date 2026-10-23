use super::*;

#[test]
fn decode_roundtrip_all_devices() {
    for i in 0..1000u32 {
        let (usage, pressed) = encode_key(i);
        let vkey = VK_F13 + u16::from(usage - crate::report::USAGE_F13);
        assert_eq!(decode_key(vkey, !pressed), Some(i % KEY_PERIOD), "key {i}");

        assert_eq!(
            decode_rel(i32::from(encode_rel(i))),
            Some(i % REL_PERIOD),
            "rel {i}"
        );

        let x = encode_abs(i);
        let normalized = (i32::from(x) * 65535) / 32767;
        assert_eq!(decode_abs(normalized), Some(i % ABS_PERIOD), "abs {i}");

        assert_eq!(
            decode_pad_lx(encode_pad_lx(i)),
            Some(i % PAD_PERIOD),
            "pad {i}"
        );
        assert_eq!(
            decode_xinput_lx(encode_xinput_lx(i)),
            Some(i % PAD_PERIOD),
            "xinput {i}"
        );
        let v = f64::from(encode_pad_lx(i)) / 65535.0;
        assert_eq!(decode_wgi_axis(v), Some(i % PAD_PERIOD), "wgi {i}");
    }
}

#[test]
fn decode_pad_rejects_center() {
    // A polled receiver's first reading is the centered stick: it must not be a code.
    assert_eq!(decode_pad_lx(0x8000), None);
    assert_eq!(decode_xinput_lx(0), None);
    assert_eq!(decode_wgi_axis(0.5), None);
    assert_eq!(decode_pad_lx(0x7FFF), Some(0x7FFF));
}

#[test]
fn input_for_matches_the_codes() {
    use crate::input::Input;
    assert_eq!(
        input_for(Probe::Kbd, 3),
        Input::Key {
            usage: 0x69,
            down: false
        }
    );
    assert_eq!(input_for(Probe::Rel, 3), Input::MouseMove { dx: -2, dy: 0 });
    assert_eq!(
        input_for(Probe::Abs, 2),
        Input::MouseAbs { x: 1060, y: 16384 }
    );
    match input_for(Probe::Pad, 70_000) {
        Input::Pad(p) => assert_eq!(p.lx, (70_000 % 32_768) as u16),
        other => panic!("{other:?}"),
    }
    assert_eq!(Probe::parse("wgi"), Some(Probe::Wgi));
    assert_eq!(Probe::parse("ABS"), Some(Probe::Abs));
    assert_eq!(Probe::parse("x"), None);
    assert_eq!(Probe::Kbd.period(), KEY_PERIOD);
    assert_eq!(Probe::Wgi.period(), PAD_PERIOD);
}

#[test]
fn decode_rejects_values_outside_the_code() {
    assert_eq!(decode_key(0x41, false), None, "'A' is not F13..F24");
    assert_eq!(decode_rel(0), None);
    assert_eq!(decode_rel(101), None);
    assert_eq!(decode_abs(10), None, "below the first code point");
    assert_eq!(decode_abs(65535), None, "above the last code point");
}

#[test]
fn abs_decode_tolerates_scaling() {
    let x = i32::from(encode_abs(500));
    let normalized = x * 65535 / 32767;
    for delta in -2..=2 {
        assert_eq!(
            decode_abs(normalized + delta * 2),
            Some(500),
            "delta {delta}"
        );
    }
}

#[test]
fn decode_ignores_foreign_device() {
    let mut filter = DeviceFilter::new();
    assert!(filter.add(0x1234));
    assert!(filter.accepts(0x1234));
    assert!(!filter.accepts(0x9999));
    assert!(!filter.accepts(0), "null handle only when added");
    assert!(filter.add(0));
    assert!(filter.accepts(0));
}

#[test]
fn filter_holds_at_most_four() {
    let mut filter = DeviceFilter::new();
    for h in 1..=4 {
        assert!(filter.add(h));
    }
    assert!(!filter.add(5));
    assert!(!filter.accepts(5));
}
