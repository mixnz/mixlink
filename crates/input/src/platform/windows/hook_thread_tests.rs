use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use super::*;

#[test]
fn starts_and_stops() {
    let thread = HookThread::start(Box::new(|_| false)).unwrap();
    assert_eq!(thread.stop(), HookCounts::default());
}

/// By hand: `cargo test -p mixlink-input hook_thread_sees_typed -- --ignored --nocapture`, type for 10 s.
#[test]
#[ignore = "needs a person typing"]
fn hook_thread_sees_typed() {
    let seen = Arc::new(AtomicU64::new(0));
    let counter = Arc::clone(&seen);
    let thread = HookThread::start(Box::new(move |_| {
        counter.fetch_add(1, Ordering::Relaxed);
        false
    }))
    .unwrap();
    std::thread::sleep(std::time::Duration::from_secs(10));
    let counts = thread.stop();
    eprintln!("sink saw {}, hook {counts:?}", seen.load(Ordering::Relaxed));
}
