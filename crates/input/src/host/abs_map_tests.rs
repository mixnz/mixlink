use super::*;

fn rect(x: i32, y: i32, w: u32, h: u32) -> Rect {
    Rect { x, y, w, h }
}

#[test]
fn whole_desktop() {
    let map = AbsMap::new((1920, 1080), None);
    assert_eq!(map.map(0, 0), (0, 0));
    assert_eq!(map.map(1919, 1079), (32767, 32767));
}

#[test]
fn extend_left_monitor() {
    let place = DesktopPlace {
        monitor: rect(0, 0, 1920, 1080),
        desktop: rect(0, 0, 4480, 1440),
    };
    let map = AbsMap::new((1920, 1080), Some(place));
    assert_eq!(map.map(1919, 0), (14038, 0));
    assert_eq!(map.map(0, 1079), (0, 24569));
}

#[test]
fn negative_desktop_origin() {
    let place = DesktopPlace {
        monitor: rect(0, 0, 1920, 1080),
        desktop: rect(-2560, 0, 4480, 1440),
    };
    let map = AbsMap::new((1920, 1080), Some(place));
    let (x, _) = map.map(0, 0);
    assert_eq!(x, 18728);
    assert!(x > 16383);
}

#[test]
fn outside_stream_clamps() {
    let map = AbsMap::new((1920, 1080), None);
    assert_eq!(map.map(u16::MAX, u16::MAX), (32767, 32767));
}

#[test]
fn stream_scaled_to_monitor() {
    let place = DesktopPlace {
        monitor: rect(0, 0, 3840, 2160),
        desktop: rect(0, 0, 3840, 2160),
    };
    let map = AbsMap::new((1920, 1080), Some(place));
    assert_eq!(map.map(960, 540), (16387, 16391));
}
