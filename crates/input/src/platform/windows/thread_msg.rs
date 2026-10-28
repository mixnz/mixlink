//! A thread that waits on its message queue (spec 0021): the client `stream-input` thread sleeps here
//! until another thread wakes it, a deadline passes (high-resolution timer, not the 15.6 ms tick), or a
//! message for it arrives. Low-level hook calls run inside the wait, on this thread.

use std::marker::PhantomData;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::time::Instant;

use windows::Win32::Foundation::{HANDLE, LPARAM, WAIT_FAILED, WPARAM};
use windows::Win32::System::Threading::{
    CREATE_WAITABLE_TIMER_HIGH_RESOLUTION, CreateWaitableTimerExW, GetCurrentThreadId, INFINITE,
    SetWaitableTimer, TIMER_ALL_ACCESS,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, MSG, MWMO_INPUTAVAILABLE, MsgWaitForMultipleObjectsEx, PM_NOREMOVE,
    PM_REMOVE, PeekMessageW, PostThreadMessageW, QS_ALLINPUT, WM_APP, WM_QUIT, WM_USER,
};
use windows::core::PCWSTR;

/// Wakes or ends a `Pump::wait` on another thread.
#[derive(Debug, Clone)]
pub struct Waker {
    thread_id: u32,
}

impl Waker {
    /// `false` when the thread is gone.
    pub fn wake(&self) -> bool {
        self.post(WM_APP)
    }

    /// The next `wait` returns `Quit`.
    pub fn quit(&self) -> bool {
        self.post(WM_QUIT)
    }

    fn post(&self, msg: u32) -> bool {
        // SAFETY: plain message post to a thread id; fails harmlessly when the thread is gone.
        unsafe { PostThreadMessageW(self.thread_id, msg, WPARAM(0), LPARAM(0)) }.is_ok()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pumped {
    Woken,
    Timeout,
    Quit,
}

/// The calling thread's message queue and deadline timer. Not `Send`: it waits for this thread only.
#[derive(Debug)]
pub struct Pump {
    thread_id: u32,
    timer: OwnedHandle,
    _thread: PhantomData<*const ()>,
}

impl Pump {
    /// Creates the thread's message queue (so wakes posted from now on are kept) and the timer.
    pub fn new() -> windows::core::Result<Pump> {
        let mut msg = MSG::default();
        // SAFETY: valid MSG out pointer; PM_NOREMOVE only makes the system create the queue.
        let _ = unsafe { PeekMessageW(&mut msg, None, WM_USER, WM_USER, PM_NOREMOVE) };
        // SAFETY: no arguments.
        let thread_id = unsafe { GetCurrentThreadId() };
        let timer = create_timer()?;
        Ok(Pump {
            thread_id,
            timer,
            _thread: PhantomData,
        })
    }

    pub fn waker(&self) -> Waker {
        Waker {
            thread_id: self.thread_id,
        }
    }

    /// Dispatches messages (hook calls among them) until a wake, `deadline` (`None` = none) or quit.
    pub fn wait(&self, deadline: Option<Instant>) -> Pumped {
        loop {
            if let Some(pumped) = self.drain() {
                return pumped;
            }
            let handle = HANDLE(self.timer.as_raw_handle());
            let handles = [handle];
            let waited = match deadline {
                Some(at) => {
                    let left = at.saturating_duration_since(Instant::now());
                    if left.is_zero() {
                        return Pumped::Timeout;
                    }
                    // Relative due time in 100 ns units, negative.
                    let due = -i64::try_from(left.as_nanos().div_ceil(100)).unwrap_or(i64::MAX);
                    // SAFETY: our timer handle, a valid due time, no completion routine.
                    if unsafe { SetWaitableTimer(handle, &due, 0, None, None, false) }.is_err() {
                        return Pumped::Timeout;
                    }
                    // SAFETY: one valid handle owned by `self`; returns on the timer or any message.
                    unsafe {
                        MsgWaitForMultipleObjectsEx(
                            Some(&handles),
                            INFINITE,
                            QS_ALLINPUT,
                            MWMO_INPUTAVAILABLE,
                        )
                    }
                }
                // SAFETY: no handles; returns on any message.
                None => unsafe {
                    MsgWaitForMultipleObjectsEx(None, INFINITE, QS_ALLINPUT, MWMO_INPUTAVAILABLE)
                },
            };
            if waited == WAIT_FAILED {
                return Pumped::Quit;
            }
        }
    }

    /// Handles every queued message; `Some` when one of them ends the wait.
    fn drain(&self) -> Option<Pumped> {
        let mut woken = false;
        let mut msg = MSG::default();
        // SAFETY: valid MSG out pointer; any window of this thread, remove what is read.
        while unsafe { PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE) }.as_bool() {
            match msg.message {
                WM_QUIT => return Some(Pumped::Quit),
                WM_APP if msg.hwnd.is_invalid() => woken = true,
                _ => {
                    // SAFETY: a message just read by PeekMessageW on this thread.
                    unsafe { DispatchMessageW(&msg) };
                }
            }
        }
        woken.then_some(Pumped::Woken)
    }
}

fn create_timer() -> windows::core::Result<OwnedHandle> {
    let access = TIMER_ALL_ACCESS.0;
    // SAFETY: unnamed timer, default security; high resolution needs Windows 10 1803+.
    let handle = unsafe {
        CreateWaitableTimerExW(
            None,
            PCWSTR::null(),
            CREATE_WAITABLE_TIMER_HIGH_RESOLUTION,
            access,
        )
    }
    // SAFETY: as above, ordinary timer as the fallback.
    .or_else(|_| unsafe { CreateWaitableTimerExW(None, PCWSTR::null(), 0, access) })?;
    // SAFETY: a fresh handle we own; `OwnedHandle` closes it once.
    Ok(unsafe { OwnedHandle::from_raw_handle(handle.0) })
}

#[cfg(test)]
#[path = "thread_msg_tests.rs"]
mod tests;
