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

        assert_eq!(decode_pad_lx(encode_pad_lx(i)), i % PAD_PERIOD, "pad {i}");
        assert_eq!(
            decode_xinput_lx(encode_xinput_lx(i)),
            i % PAD_PERIOD,
            "xinput {i}"
        );
        let v = f64::from(encode_pad_lx(i)) / 65535.0;
        assert_eq!(decode_wgi_axis(v), i % PAD_PERIOD, "wgi {i}");
    }
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

#[test]
fn unwrap_never_goes_back() {
    let mut u = Unwrapper::new(24);
    assert_eq!(u.next(0), Some(0));
    assert_eq!(u.next(1), Some(1));
    assert_eq!(u.next(2), Some(2));
    assert_eq!(u.next(1), None, "late event");
    assert_eq!(u.next(2), None, "duplicate");
    assert_eq!(u.next(3), Some(3));
}

#[test]
fn unwrap_crosses_the_period() {
    let mut u = Unwrapper::new(24);
    for i in 0..24 {
        assert_eq!(u.next(i), Some(i));
    }
    assert_eq!(u.next(0), Some(24));
    assert_eq!(u.next(2), Some(26), "one lost event in between");
}
