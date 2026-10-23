//! Frame pacing: high-resolution waitable timer for the bulk of the wait, spin for the last 0.5 ms.

use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::Threading::{
    CREATE_WAITABLE_TIMER_HIGH_RESOLUTION, CreateWaitableTimerExW, INFINITE, SetWaitableTimer,
    TIMER_ALL_ACCESS, WaitForSingleObject,
};

use crate::qpc;
use crate::util::BoxResult;

pub struct Pacer {
    timer: HANDLE,
}

impl Pacer {
    pub fn new() -> BoxResult<Self> {
        // SAFETY: plain timer creation; the handle is closed in Drop.
        let timer = unsafe {
            CreateWaitableTimerExW(
                None,
                None,
                CREATE_WAITABLE_TIMER_HIGH_RESOLUTION,
                TIMER_ALL_ACCESS.0,
            )?
        };
        Ok(Self { timer })
    }

    /// Returns at QPC `deadline` (or at once if it has passed).
    pub fn wait_until(&self, deadline: u64) {
        let spin_ticks = qpc::freq() / 2_000;
        let now = qpc::now();
        if deadline > now + spin_ticks {
            let sleep_100ns =
                (deadline - now - spin_ticks) as u128 * 10_000_000 / u128::from(qpc::freq());
            let due = -(sleep_100ns as i64);
            // SAFETY: the timer handle is alive; relative due time in 100 ns units.
            unsafe {
                if SetWaitableTimer(self.timer, &due, 0, None, None, false).is_ok() {
                    WaitForSingleObject(self.timer, INFINITE);
                }
            }
        }
        while qpc::now() < deadline {
            std::hint::spin_loop();
        }
    }
}

impl Drop for Pacer {
    fn drop(&mut self) {
        // SAFETY: handle owned here.
        unsafe {
            let _ = CloseHandle(self.timer);
        }
    }
}
