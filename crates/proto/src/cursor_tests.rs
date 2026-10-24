use super::*;
use crate::channel::Channel;
use crate::reassembler::ReassemblerConfig;

fn shape(kind: ShapeKind, width: u16, height: u16) -> CursorShapeHeader {
    CursorShapeHeader {
        shape_id: 0xA1B2_C3D4,
        kind,
        width,
        height,
        hot_x: 3,
        hot_y: 5,
        pitch: u32::from(width) * 4,
    }
}

fn encoded_shape(header: &CursorShapeHeader, pixels: &[u8]) -> Vec<u8> {
    let mut out = vec![0u8; CURSOR_SHAPE_HEADER_LEN + pixels.len()];
    let len = header.encode_into(pixels, &mut out).unwrap();
    assert_eq!(len, out.len());
    out
}

#[test]
fn position_round_trips() {
    let pos = CursorPosition {
        x: -12,
        y: -3400,
        visible: true,
        shape_id: 77,
    };
    let mut buf = [0u8; CURSOR_POSITION_LEN];
    pos.encode(&mut buf);
    assert_eq!(buf[0], 1);
    assert_eq!(buf[1..5], (-12i32).to_le_bytes());
    assert_eq!(decode(&buf), Ok(CursorMessage::Position(pos)));
}

#[test]
fn shape_round_trips_with_borrowed_pixels() {
    let header = shape(ShapeKind::Color, 32, 32);
    let pixels: Vec<u8> = (0..32 * 32 * 4).map(|i| i as u8).collect();
    let bytes = encoded_shape(&header, &pixels);
    assert_eq!(bytes[0], 2);
    let Ok(CursorMessage::Shape {
        header: h,
        pixels: p,
    }) = decode(&bytes)
    else {
        panic!("expected shape");
    };
    assert_eq!(h, header);
    assert_eq!(p, &pixels[..]);
    assert_eq!(p.as_ptr(), bytes[CURSOR_SHAPE_HEADER_LEN..].as_ptr());
}

#[test]
fn truncated_buffers_are_too_short() {
    let mut pos = [0u8; CURSOR_POSITION_LEN];
    CursorPosition {
        x: 1,
        y: 2,
        visible: false,
        shape_id: 3,
    }
    .encode(&mut pos);
    for len in 0..CURSOR_POSITION_LEN {
        assert_eq!(decode(&pos[..len]), Err(CursorError::TooShort { len }));
    }
    let bytes = encoded_shape(&shape(ShapeKind::Color, 1, 1), &[0; 4]);
    for len in 1..CURSOR_SHAPE_HEADER_LEN {
        assert_eq!(decode(&bytes[..len]), Err(CursorError::TooShort { len }));
    }
}

#[test]
fn bad_bytes_are_rejected() {
    assert_eq!(decode(&[9; 20]), Err(CursorError::Kind { byte: 9 }));
    let mut pos = [0u8; CURSOR_POSITION_LEN];
    pos[0] = 1;
    pos[9] = 2;
    assert_eq!(decode(&pos), Err(CursorError::Visible { byte: 2 }));
    let mut bytes = encoded_shape(&shape(ShapeKind::Color, 1, 1), &[0; 4]);
    bytes[5] = 4;
    assert_eq!(decode(&bytes), Err(CursorError::ShapeKind { byte: 4 }));
}

#[test]
fn pixel_length_must_match_pitch_times_height() {
    let header = shape(ShapeKind::Color, 4, 4);
    let mut out = [0u8; 256];
    assert_eq!(
        header.encode_into(&[0; 60], &mut out),
        Err(CursorError::PixelLen {
            expected: 64,
            found: 60
        })
    );
    let mut bytes = encoded_shape(&header, &[0; 64]);
    bytes.pop();
    assert_eq!(
        decode(&bytes),
        Err(CursorError::PixelLen {
            expected: 64,
            found: 63
        })
    );
}

#[test]
fn monochrome_height_must_be_even() {
    let mut header = shape(ShapeKind::Monochrome, 32, 3);
    header.pitch = 4;
    let mut out = [0u8; 64];
    assert_eq!(
        header.encode_into(&[0; 12], &mut out),
        Err(CursorError::MonochromeHeight { height: 3 })
    );
    header.height = 64;
    let bytes = encoded_shape(&header, &[0; 256]);
    assert!(matches!(decode(&bytes), Ok(CursorMessage::Shape { .. })));
}

#[test]
fn small_output_buffer_is_rejected() {
    let header = shape(ShapeKind::MaskedColor, 2, 2);
    let mut out = [0u8; 20];
    assert_eq!(
        header.encode_into(&[0; 16], &mut out),
        Err(CursorError::BufferTooSmall { need: 34, have: 20 })
    );
}

#[test]
fn largest_shape_fits_cursor_reassembler() {
    let max = ReassemblerConfig::for_channel(Channel::Cursor).max_message_len;
    assert!(CURSOR_SHAPE_HEADER_LEN + 256 * 256 * 4 <= max);
}
