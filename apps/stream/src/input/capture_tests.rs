use super::*;

#[test]
fn centre_maps_to_centre() {
    assert_eq!(
        to_stream((640.0, 360.0), (1280, 720), (1920, 1080)),
        Some((960, 540))
    );
}

#[test]
fn letterbox_bar_clamps_to_edge() {
    // 1280x1024 window, 16:9 video 1280x720 at y 152..872.
    assert_eq!(
        to_stream((640.0, 10.0), (1280, 1024), (1920, 1080)),
        Some((960, 0))
    );
    assert_eq!(
        to_stream((640.0, 1000.0), (1280, 1024), (1920, 1080)),
        Some((960, 1079))
    );
}

#[test]
fn far_edge_clamps() {
    assert_eq!(
        to_stream((1280.0, 720.0), (1280, 720), (1920, 1080)),
        Some((1919, 1079))
    );
    assert_eq!(
        to_stream((-5.0, -5.0), (1280, 720), (1920, 1080)),
        Some((0, 0))
    );
}

#[test]
fn empty_window_is_none() {
    assert_eq!(to_stream((1.0, 1.0), (0, 0), (1920, 1080)), None);
    assert_eq!(to_stream((1.0, 1.0), (1280, 720), (0, 0)), None);
}
