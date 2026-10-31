use super::*;

#[test]
fn corners_map_to_the_full_range() {
    let map = AbsMap::new((1920, 1080));
    assert_eq!(map.map(0, 0), (0, 0));
    assert_eq!(map.map(1919, 1079), (32767, 32767));
}

#[test]
fn centre_maps_to_half() {
    let map = AbsMap::new((1920, 1080));
    assert_eq!(map.map(960, 540), (16392, 16398));
}

#[test]
fn outside_stream_clamps() {
    let map = AbsMap::new((1920, 1080));
    assert_eq!(map.map(u16::MAX, u16::MAX), (32767, 32767));
}

#[test]
fn empty_stream_does_not_divide_by_zero() {
    let map = AbsMap::new((0, 0));
    assert_eq!(map.map(5, 5), (32767, 32767));
}
