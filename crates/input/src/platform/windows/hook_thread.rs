//! The keyboard hook on a thread of its own (spec 0021), waiting in a plain `GetMessageW` loop as
//! Windows documents for low-level hooks. Waiting in `MsgWaitForMultipleObjectsEx` instead, the hook
//! was never called on the client laptop while the stream window (same process) was in front, and
//! every other keyboard hook on the desktop was starved with it.

use std::cell::RefCell;
use std::io;
use std::sync::mpsc;
use std::thread::{self, JoinHandle};

use mixlink_core::mmcss::{Mmcss, MmcssTask};
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, MSG, PM_NOREMOVE, PeekMessageW, PostThreadMessageW, WM_QUIT,
    WM_USER,
};

use super::kbd_hook::{HookCounts, KeyHook, RawKey};

/// Decides each key on the hook thread: `true` swallows it. Must return fast (no blocking, no I/O).
pub type KeySink = Box<dyn FnMut(RawKey) -> bool + Send>;

thread_local! {
    static SINK: RefCell<Option<KeySink>> = const { RefCell::new(None) };
}

/// The running hook thread; `stop` removes the hook and returns what it saw.
#[derive(Debug)]
pub struct HookThread {
    thread_id: u32,
    handle: JoinHandle<HookCounts>,
}

impl HookThread {
    pub fn start(sink: KeySink) -> io::Result<HookThread> {
        let (ready, started) = mpsc::sync_channel(1);
        let handle = thread::Builder::new()
            .name("input-hook".into())
            .spawn(move || run(sink, &ready))?;
        match started.recv() {
            Ok(Ok(thread_id)) => Ok(HookThread { thread_id, handle }),
            Ok(Err(message)) => {
                let _ = handle.join();
                Err(io::Error::other(message))
            }
            Err(_) => Err(io::Error::other("keyboard hook thread ended at start")),
        }
    }

    pub fn stop(self) -> HookCounts {
        // SAFETY: plain message post; the thread ends its `GetMessageW` loop on `WM_QUIT`.
        let _ = unsafe { PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0)) };
        self.handle.join().unwrap_or_default()
    }
}

fn run(sink: KeySink, ready: &mpsc::SyncSender<Result<u32, String>>) -> HookCounts {
    let _mmcss = Mmcss::enter(MmcssTask::Games);
    let mut msg = MSG::default();
    // SAFETY: valid MSG out pointer; PM_NOREMOVE only makes the system create the queue, so a
    // `WM_QUIT` posted right after start is kept.
    let _ = unsafe { PeekMessageW(&mut msg, None, WM_USER, WM_USER, PM_NOREMOVE) };
    SINK.with(|s| *s.borrow_mut() = Some(sink));
    let hook = match KeyHook::install(call_sink) {
        Ok(hook) => hook,
        Err(error) => {
            let _ = ready.send(Err(error.to_string()));
            return HookCounts::default();
        }
    };
    // SAFETY: no arguments.
    let _ = ready.send(Ok(unsafe { GetCurrentThreadId() }));
    // SAFETY: valid MSG out pointer; 0 = `WM_QUIT`, -1 = error: both end the loop. Hook calls are
    // made inside `GetMessageW`.
    while unsafe { GetMessageW(&mut msg, None, 0, 0) }.0 > 0 {
        // SAFETY: a message just read by GetMessageW on this thread.
        unsafe { DispatchMessageW(&msg) };
    }
    let counts = hook.counts();
    drop(hook);
    SINK.with(|s| s.borrow_mut().take());
    counts
}

// HOT PATH
fn call_sink(key: RawKey) -> bool {
    SINK.with(|s| {
        s.try_borrow_mut()
            .ok()
            .and_then(|mut sink| sink.as_mut().map(|f| f(key)))
            .unwrap_or(false)
    })
}

#[cfg(test)]
#[path = "hook_thread_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "raw_probe_tests.rs"]
mod raw_probe;
