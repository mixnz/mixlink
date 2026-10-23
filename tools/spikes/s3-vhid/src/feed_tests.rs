use super::*;

#[test]
fn parse_hex_bytes() {
    assert_eq!(parse_hex("0509"), Some(vec![0x05, 0x09]));
    assert_eq!(parse_hex("05 09 ff"), Some(vec![0x05, 0x09, 0xFF]));
    assert_eq!(parse_hex("AbC0"), Some(vec![0xAB, 0xC0]));
}

#[test]
fn parse_hex_rejects_bad_input() {
    assert_eq!(parse_hex(""), None);
    assert_eq!(parse_hex("050"), None, "odd digit count");
    assert_eq!(parse_hex("zz"), None);
}

#[test]
fn backend_names() {
    assert_eq!(Backend::parse("vhid"), Some(Backend::Vhid));
    assert_eq!(Backend::parse("SendInput"), Some(Backend::SendInput));
    assert_eq!(Backend::parse("vigem"), Some(Backend::Vigem));
    assert_eq!(Backend::parse("dinput"), None);
}
