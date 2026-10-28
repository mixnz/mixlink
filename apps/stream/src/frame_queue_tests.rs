use super::*;

#[test]
fn oldest_first() {
    let mut q = FrameQueue::new();
    assert!(q.push(1).is_none());
    assert!(q.push(2).is_none());
    assert_eq!(q.pop(), Some(1));
    assert_eq!(q.pop(), Some(2));
    assert_eq!(q.pop(), None);
}

#[test]
fn full_queue_gives_up_the_oldest() {
    let mut q = FrameQueue::new();
    q.push(1);
    q.push(2);
    assert_eq!(
        q.push(3),
        Some(1),
        "two frames in one refresh are both shown, a third is not"
    );
    assert_eq!(q.len(), 2);
    assert_eq!(q.pop(), Some(2));
    assert_eq!(q.pop(), Some(3));
}

#[test]
fn two_frames_in_one_refresh_are_both_shown() {
    // 60 fps in, 60 Hz out, two arrivals bunched into one refresh: nothing dropped.
    let mut q = FrameQueue::new();
    let mut shown = Vec::new();
    for refresh in 0..6 {
        match refresh {
            2 => {}
            1 => {
                q.push(10);
                q.push(11);
            }
            n => {
                q.push(n * 10);
            }
        }
        shown.extend(q.pop());
    }
    assert_eq!(shown, vec![0, 10, 11, 30, 40, 50]);
}
