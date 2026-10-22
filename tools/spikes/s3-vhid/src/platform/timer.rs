//! High-resolution waitable timer for pacing the sender.

use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::Threading::{
    CREATE_WAITABLE_TIMER_HIGH_RESOLUTION, CreateWaitableTimerExW, INFINITE, SetWaitableTimer,
    TIMER_ALL_ACCESS, WaitForSingleObject,
};

use super::os_err;
use crate::feed::FeedError;

pub struct Pacer {
    timer: HANDLE,
}

impl Pacer {
    pub fn new() -> Result<Self, FeedError> {
        // SAFETY: unnamed timer, no security attributes.
        let timer = unsafe {
            CreateWaitableTimerExW(
                None,
                None,
                CREATE_WAITABLE_TIMER_HIGH_RESOLUTION,
                TIMER_ALL_ACCESS.0,
            )
        }
        .map_err(|e| os_err("CreateWaitableTimerExW", e))?;
        Ok(Self { timer })
    }

    /// Sleeps `micros` from now.
    pub fn sleep_us(&self, micros: i64) -> Result<(), FeedError> {
        let due = -(micros * 10); // relative, 100 ns units
        // SAFETY: valid timer handle and due-time pointer; no completion routine.
        unsafe { SetWaitableTimer(self.timer, &due, 0, None, None, false) }
            .map_err(|e| os_err("SetWaitableTimer", e))?;
        // SAFETY: valid timer handle.
        unsafe { WaitForSingleObject(self.timer, INFINITE) };
        Ok(())
    }
}

impl Drop for Pacer {
    fn drop(&mut self) {
        // SAFETY: handle from CreateWaitableTimerExW, closed once.
        let _ = unsafe { CloseHandle(self.timer) };
    }
}
