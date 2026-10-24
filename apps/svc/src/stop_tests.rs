use std::sync::Arc;
use std::thread;
use std::time::Duration;

use super::*;

#[test]
fn wait_timeout_false_before_stop() {
    let stop = StopSignal::new();
    assert!(!stop.wait_timeout(Duration::from_millis(10)));
    assert!(!stop.is_stopped());
}

#[test]
fn stop_from_another_thread_wakes_wait() {
    let stop = Arc::new(StopSignal::new());
    let other = Arc::clone(&stop);
    let handle = thread::spawn(move || {
        thread::sleep(Duration::from_millis(20));
        other.stop();
    });
    stop.wait();
    assert!(stop.is_stopped());
    assert!(stop.wait_timeout(Duration::from_millis(1)));
    handle.join().unwrap();
}
