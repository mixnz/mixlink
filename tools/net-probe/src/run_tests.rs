use super::*;

#[test]
fn abandoned_frames_do_not_reuse_frame_ids() {
    let mut report = SendReport::default();
    let first = report.next_frame_id();
    report.abandoned += 1;
    let second = report.next_frame_id();
    report.frames += 1;
    let third = report.next_frame_id();
    assert_eq!(first, FrameId::new(0));
    assert_eq!(second, FrameId::new(1));
    assert_eq!(third, FrameId::new(2));
}
