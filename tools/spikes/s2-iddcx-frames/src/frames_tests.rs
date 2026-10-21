use super::*;

#[test]
fn reference_uses_acquire_when_present_later() {
    assert_eq!(reference_qpc(200, 150), (150, RefKind::Acquire));
}

#[test]
fn reference_uses_present_when_not_later() {
    assert_eq!(reference_qpc(100, 150), (100, RefKind::Present));
    assert_eq!(reference_qpc(150, 150), (150, RefKind::Present));
}

#[test]
fn lost_counts_gaps() {
    let mut t = FrameTracker::new(16);
    for n in [5, 6, 9] {
        t.on_frame(Some(n), true, 10);
    }
    let s = t.summary();
    assert_eq!((s.delivered, s.lost), (3, 2));
}

#[test]
fn lost_counts_overwritten_slot() {
    let mut t = FrameTracker::new(16);
    t.on_frame(Some(5), true, 10);
    t.on_frame(Some(6), false, 99);
    t.on_frame(Some(7), true, 20);
    let s = t.summary();
    assert_eq!((s.delivered, s.lost, s.overwritten), (2, 1, 1));
    assert_eq!(t.latencies_mut(), &mut [10, 20]);
}

#[test]
fn duplicates_are_not_lost_nor_sampled() {
    let mut t = FrameTracker::new(16);
    for n in [5, 5, 6] {
        t.on_frame(Some(n), true, 10);
    }
    let s = t.summary();
    assert_eq!((s.delivered, s.lost, s.duplicates), (2, 0, 1));
    assert_eq!(t.latencies_mut().len(), 2);
}

#[test]
fn undecodable_frame_is_counted_not_sampled() {
    let mut t = FrameTracker::new(16);
    t.on_frame(Some(5), true, 10);
    t.on_frame(None, true, 10);
    t.on_frame(Some(6), true, 10);
    let s = t.summary();
    assert_eq!((s.delivered, s.lost, s.undecodable), (2, 0, 1));
}

#[test]
fn counter_restart_is_not_lost() {
    let mut t = FrameTracker::new(16);
    t.on_frame(Some(10), true, 10);
    t.on_frame(Some(3), true, 10);
    assert_eq!(t.summary().lost, 0);
}

#[test]
fn samples_beyond_capacity_are_counted_as_overflow() {
    let mut t = FrameTracker::new(2);
    for n in 1..=3 {
        t.on_frame(Some(n), true, 10);
    }
    let s = t.summary();
    assert_eq!((s.delivered, s.overflow), (3, 1));
    assert_eq!(t.latencies_mut().len(), 2);
}
