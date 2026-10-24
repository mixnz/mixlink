//! One-way stop flag shared by the run loop, the service handler and the Ctrl+C handler.
//! Control plane only: the mutex is never touched by a hot path.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Condvar, Mutex, PoisonError};
use std::time::Duration;

#[derive(Debug, Default)]
pub struct StopSignal {
    flag: AtomicBool,
    lock: Mutex<()>,
    cond: Condvar,
}

impl StopSignal {
    pub fn new() -> StopSignal {
        StopSignal::default()
    }

    pub fn stop(&self) {
        let _guard = self.lock.lock().unwrap_or_else(PoisonError::into_inner);
        self.flag.store(true, Ordering::Release);
        self.cond.notify_all();
    }

    pub fn is_stopped(&self) -> bool {
        self.flag.load(Ordering::Acquire)
    }

    /// Blocks until `stop` is called.
    pub fn wait(&self) {
        let mut guard = self.lock.lock().unwrap_or_else(PoisonError::into_inner);
        while !self.is_stopped() {
            guard = self
                .cond
                .wait(guard)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }

    /// Returns `true` if stopped within `timeout`.
    pub fn wait_timeout(&self, timeout: Duration) -> bool {
        let guard = self.lock.lock().unwrap_or_else(PoisonError::into_inner);
        let (_guard, _) = self
            .cond
            .wait_timeout_while(guard, timeout, |()| !self.is_stopped())
            .unwrap_or_else(PoisonError::into_inner);
        self.is_stopped()
    }
}

#[cfg(test)]
#[path = "stop_tests.rs"]
mod tests;
