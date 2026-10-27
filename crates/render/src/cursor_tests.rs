use super::*;

fn bgra(b: u8, g: u8, r: u8, a: u8) -> [u8; 4] {
    [b, g, r, a]
}

fn image(kind: CursorKind, width: u32, height: u32, pitch: u32, px: &[[u8; 4]]) -> CursorImage {
    let mut pixels = vec![0u8; (pitch * height) as usize];
    for (i, p) in px.iter().enumerate() {
        let (x, y) = (i as u32 % width, i as u32 / width);
        let at = (y * pitch + x * 4) as usize;
        pixels[at..at + 4].copy_from_slice(p);
    }
    convert_shape(kind, width, height, (1, 1), pitch, &pixels).unwrap()
}

#[test]
fn color_shape_premultiplied() {
    let img = image(
        CursorKind::Alpha,
        2,
        1,
        8,
        &[bgra(200, 100, 50, 128), bgra(10, 20, 30, 255)],
    );
    assert_eq!(&img.normal[..4], &[100, 50, 25, 128]);
    assert_eq!(&img.normal[4..], &[10, 20, 30, 255]);
    assert!(img.invert.is_none());
}

#[test]
fn masked_opaque_colour() {
    let img = image(CursorKind::MaskedColor, 1, 1, 4, &[bgra(1, 2, 3, 0)]);
    assert_eq!(img.normal, vec![1, 2, 3, 255]);
    assert!(img.invert.is_none());
}

#[test]
fn masked_black_xor_is_transparent() {
    let img = image(CursorKind::MaskedColor, 1, 1, 4, &[bgra(0, 0, 0, 0xFF)]);
    assert_eq!(img.normal, vec![0, 0, 0, 0]);
    assert!(img.invert.is_none());
}

#[test]
fn masked_white_xor_goes_to_invert() {
    let img = image(
        CursorKind::MaskedColor,
        2,
        1,
        8,
        &[bgra(255, 255, 255, 0xFF), bgra(9, 9, 9, 0)],
    );
    assert_eq!(&img.normal[..4], &[0, 0, 0, 0]);
    assert_eq!(&img.normal[4..], &[9, 9, 9, 255]);
    assert_eq!(img.invert, Some(vec![255, 255, 255, 255, 0, 0, 0, 0]));
}

#[test]
fn pitch_padding_ignored() {
    let img = image(
        CursorKind::Alpha,
        1,
        2,
        16,
        &[bgra(1, 1, 1, 255), bgra(2, 2, 2, 255)],
    );
    assert_eq!(img.normal, vec![1, 1, 1, 255, 2, 2, 2, 255]);
    assert_eq!((img.width, img.height, img.hot), (1, 2, (1, 1)));
}

#[test]
fn short_buffer_is_error() {
    assert!(matches!(
        convert_shape(CursorKind::Alpha, 4, 4, (0, 0), 16, &[0; 63]),
        Err(RenderError::CursorShape { .. })
    ));
    assert!(matches!(
        convert_shape(CursorKind::Alpha, 4, 4, (0, 0), 8, &[0; 64]),
        Err(RenderError::CursorShape { .. }),
    ));
    assert!(matches!(
        convert_shape(CursorKind::Alpha, 0, 4, (0, 0), 16, &[0; 64]),
        Err(RenderError::CursorShape { .. }),
    ));
    assert!(matches!(
        convert_shape(CursorKind::Alpha, 257, 1, (0, 0), 1028, &[0; 1028]),
        Err(RenderError::CursorShape { .. }),
    ));
}

#[test]
fn rect_at_one_to_one() {
    // 1920x1080 video in a 1920x1080 window: the hotspot lands on the position.
    let r = cursor_rect((1920, 1080), (1920, 1080), (32, 32), (4, 6), (100, 200));
    assert_eq!(
        r,
        Rect {
            x: 96.0,
            y: 194.0,
            w: 32.0,
            h: 32.0
        }
    );
}

#[test]
fn rect_scales_with_letterbox() {
    // 1920x1080 video in a 960x1080 window: half size, 270 px bar on top.
    let r = cursor_rect((1920, 1080), (960, 1080), (32, 32), (0, 0), (1920, 0));
    assert_eq!(
        r,
        Rect {
            x: 960.0,
            y: 270.0,
            w: 16.0,
            h: 16.0
        }
    );
}

#[test]
fn rect_empty_without_video() {
    let r = cursor_rect((0, 0), (960, 1080), (32, 32), (0, 0), (10, 10));
    assert_eq!(r, Rect::default());
}
