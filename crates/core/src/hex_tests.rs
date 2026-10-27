use super::*;

#[test]
fn encode_is_lowercase() {
    assert_eq!(encode(&[0x00, 0xAB, 0x7f]), "00ab7f");
}

#[test]
fn decode_into_round_trips() {
    let mut out = [0u8; 3];
    decode_into("00ab7f", &mut out).unwrap();
    assert_eq!(out, [0x00, 0xab, 0x7f]);
}

#[test]
fn uppercase_accepted() {
    let mut out = [0u8; 2];
    decode_into("ABcd", &mut out).unwrap();
    assert_eq!(out, [0xab, 0xcd]);
}

#[test]
fn wrong_length_rejected() {
    let mut out = [0u8; 2];
    assert_eq!(
        decode_into("abc", &mut out),
        Err(HexError::Length {
            expected: 4,
            len: 3
        })
    );
}

#[test]
fn non_hex_rejected() {
    let mut out = [0u8; 2];
    assert_eq!(
        decode_into("ab0g", &mut out),
        Err(HexError::NotHex { index: 3 })
    );
}

#[test]
fn decode_vec_round_trips() {
    assert_eq!(decode_vec("0102ff", 8).unwrap(), vec![1, 2, 0xff]);
}

#[test]
fn decode_vec_odd_length_rejected() {
    assert_eq!(decode_vec("012", 8), Err(HexError::OddLength { len: 3 }));
}

#[test]
fn decode_vec_too_long_rejected() {
    assert_eq!(
        decode_vec("010203", 2),
        Err(HexError::TooLong { max: 2, len: 3 })
    );
}
