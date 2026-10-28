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
    q.push(3);
    assert_eq!(q.push(4), Some(1), "three wait at most");
    assert_eq!(q.len(), 3);
    assert_eq!(q.pop(), Some(2));
    assert_eq!(q.pop(), Some(3));
    assert_eq!(q.pop(), Some(4));
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

/// 60 fps in with ±`jitter_ms` arrival jitter, shown at `refresh_hz` for `seconds`.
/// Returns (frames lost, mean frames waiting at a refresh).
fn simulate(refresh_hz: f64, jitter_ms: f64, seconds: f64) -> (u64, f64) {
    let mut q = FrameQueue::new();
    let mut seed = 0x2545_F491_u32;
    let mut jitter = || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        (f64::from(seed % 2001) / 1000.0 - 1.0) * jitter_ms
    };
    let frames = (seconds * 60.0) as u64;
    let mut arrivals = Vec::new();
    let mut last = f64::MIN;
    for k in 0..frames {
        let at = (k as f64 * 1000.0 / 60.0 + jitter()).max(last);
        arrivals.push(at);
        last = at;
    }
    let (mut next, mut lost, mut level_sum, mut refreshes) = (0usize, 0u64, 0usize, 0u64);
    let mut vblank = 0.0;
    while vblank < seconds * 1000.0 {
        while next < arrivals.len() && arrivals[next] <= vblank {
            lost += u64::from(q.push(next).is_some());
            next += 1;
        }
        level_sum += q.len();
        refreshes += 1;
        let (_, skipped) = q.pop_paced();
        lost += u64::from(skipped.is_some());
        vblank += 1000.0 / refresh_hz;
    }
    (lost, level_sum as f64 / refreshes as f64)
}

#[test]
fn steady_60_fps_on_a_59_99_hz_screen_stays_smooth() {
    // The laptop of spec 0020 Results: host 60.000 Hz, panel 59.99 Hz, a few ms of network jitter.
    let (lost, level) = simulate(59.99, 4.0, 120.0);
    assert!(
        lost <= 12,
        "lost {lost} frames in 2 minutes (one every 10 s at most)"
    );
    assert!(level < 1.6, "frames wait too long: mean level {level:.2}");
}

#[test]
fn backlog_is_drained_so_latency_stays_low() {
    // No jitter, display a hair slower than the source: the queue creeps up and is drained.
    let (_, level) = simulate(59.9, 0.0, 60.0);
    assert!(level < 1.6, "mean level {level:.2}");
}

#[test]
fn lasting_backlog_skips_one_frame() {
    let mut q = FrameQueue::new();
    q.push(0);
    let mut skipped = Vec::new();
    for n in 1..=40 {
        q.push(n);
        let (_, skip) = q.pop_paced();
        skipped.extend(skip);
    }
    assert_eq!(skipped.len(), 1, "one skip after 30 refreshes of backlog");
    assert!(q.len() <= 1);
}
