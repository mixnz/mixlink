use super::*;

const PITCH: usize = CELL_W as usize * 4 + 64; // padded rows, like a mapped staging texture

fn render(n: u32) -> Vec<u8> {
    let bits = encode(n);
    let mut buf = vec![0u8; PITCH * CELL_H as usize];
    for y in 0..CELL_H as usize {
        for x in 0..CELL_W as usize {
            let v = if bits[x / BLOCK as usize] { 255 } else { 0 };
            let p = y * PITCH + x * 4;
            buf[p..p + 4].copy_from_slice(&[v, v, v, 255]);
        }
    }
    buf
}

#[test]
fn encode_puts_bit_31_first() {
    assert!(encode(0x8000_0000)[0]);
    assert!(encode(1)[31]);
    assert!(!encode(1)[0]);
}

#[test]
fn counter_roundtrip() {
    for n in [0, 1, 0x8000_0001, 123_456, u32::MAX] {
        assert_eq!(decode(&render(n), PITCH), Some(n), "n = {n}");
    }
}

#[test]
fn decode_rejects_mid_gray() {
    let buf = vec![128u8; PITCH * CELL_H as usize];
    assert_eq!(decode(&buf, PITCH), None);
}

#[test]
fn decode_rejects_short_buffer() {
    assert_eq!(decode(&[0u8; 16], PITCH), None);
}
