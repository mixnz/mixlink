use super::{Rect, fit};

#[test]
fn same_aspect_fills() {
    assert_eq!(
        fit((1920, 1080), (1280, 720)),
        Rect {
            x: 0.0,
            y: 0.0,
            w: 1280.0,
            h: 720.0
        }
    );
}

#[test]
fn wide_source_is_letterboxed() {
    assert_eq!(
        fit((1920, 1080), (1000, 1000)),
        Rect {
            x: 0.0,
            y: 218.75,
            w: 1000.0,
            h: 562.5
        }
    );
}

#[test]
fn narrow_source_is_pillarboxed() {
    assert_eq!(
        fit((640, 480), (1920, 1080)),
        Rect {
            x: 240.0,
            y: 0.0,
            w: 1440.0,
            h: 1080.0
        }
    );
}

#[test]
fn zero_size_gives_empty_rect() {
    for (src, dst) in [
        ((0, 1080), (100, 100)),
        ((1920, 1080), (0, 100)),
        ((10, 10), (10, 0)),
    ] {
        assert_eq!(fit(src, dst), Rect::default());
    }
}
