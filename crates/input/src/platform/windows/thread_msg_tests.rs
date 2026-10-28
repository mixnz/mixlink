use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use super::*;

/// Runs `Pump::wait` on a new thread; returns its waker and the outcome with the time it took.
fn waiting_thread(
    deadline: Option<Duration>,
) -> (Waker, thread::JoinHandle<Option<(Pumped, Duration)>>) {
    let (tx, rx) = mpsc::channel();
    let handle = thread::spawn(move || {
        let pump = Pump::new().ok()?;
        let _ = tx.send(pump.waker());
        let start = Instant::now();
        let pumped = pump.wait(deadline.map(|d| start + d));
        Some((pumped, start.elapsed()))
    });
    let waker = rx.recv_timeout(Duration::from_secs(5)).unwrap();
    (waker, handle)
}

#[test]
fn wake_returns_woken() {
    let (waker, handle) = waiting_thread(None);
    thread::sleep(Duration::from_millis(10));
    assert!(waker.wake());
    let (pumped, took) = handle.join().unwrap().unwrap();
    assert_eq!(pumped, Pumped::Woken);
    assert!(took < Duration::from_secs(1), "{took:?}");
}

#[test]
fn timeout_returns_timeout() {
    let (_waker, handle) = waiting_thread(Some(Duration::from_millis(4)));
    let (pumped, took) = handle.join().unwrap().unwrap();
    assert_eq!(pumped, Pumped::Timeout);
    assert!(took >= Duration::from_millis(4), "{took:?}");
    // A high-resolution timer: far below the 15.6 ms default tick.
    assert!(took < Duration::from_millis(15), "{took:?}");
}

#[test]
fn quit_returns_quit() {
    let (waker, handle) = waiting_thread(None);
    assert!(waker.quit());
    let (pumped, _) = handle.join().unwrap().unwrap();
    assert_eq!(pumped, Pumped::Quit);
}

#[test]
fn wake_before_wait_is_not_lost() {
    let pump = Pump::new().unwrap();
    assert!(pump.waker().wake());
    let deadline = Instant::now() + Duration::from_millis(200);
    assert_eq!(pump.wait(Some(deadline)), Pumped::Woken);
}
