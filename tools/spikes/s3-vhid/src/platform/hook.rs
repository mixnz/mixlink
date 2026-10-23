//! Low-level keyboard / mouse hooks: counts events with and without the injected flags (case C).

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, KBDLLHOOKSTRUCT, LLKHF_INJECTED, LLKHF_LOWER_IL_INJECTED,
    LLMHF_INJECTED, LLMHF_LOWER_IL_INJECTED, MSG, MSLLHOOKSTRUCT, PM_REMOVE, PeekMessageW,
    SetWindowsHookExW, UnhookWindowsHookEx, WH_KEYBOARD_LL, WH_MOUSE_LL,
};

use super::os_err;
use crate::feed::FeedError;

static KEY_CLEAN: AtomicU64 = AtomicU64::new(0);
static KEY_INJECTED: AtomicU64 = AtomicU64::new(0);
static MOUSE_CLEAN: AtomicU64 = AtomicU64::new(0);
static MOUSE_INJECTED: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, Default)]
pub struct HookCounts {
    pub key_clean: u64,
    pub key_injected: u64,
    pub mouse_clean: u64,
    pub mouse_injected: u64,
}

unsafe extern "system" fn keyboard(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        // SAFETY: for WH_KEYBOARD_LL with code >= 0, lparam points to a KBDLLHOOKSTRUCT.
        let info = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
        let injected = info.flags.0 & (LLKHF_INJECTED.0 | LLKHF_LOWER_IL_INJECTED.0) != 0;
        let counter = if injected { &KEY_INJECTED } else { &KEY_CLEAN };
        counter.fetch_add(1, Ordering::Relaxed);
    }
    // SAFETY: passes the event on unchanged.
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

unsafe extern "system" fn mouse(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        // SAFETY: for WH_MOUSE_LL with code >= 0, lparam points to a MSLLHOOKSTRUCT.
        let info = unsafe { &*(lparam.0 as *const MSLLHOOKSTRUCT) };
        let injected = info.flags & (LLMHF_INJECTED | LLMHF_LOWER_IL_INJECTED) != 0;
        let counter = if injected {
            &MOUSE_INJECTED
        } else {
            &MOUSE_CLEAN
        };
        counter.fetch_add(1, Ordering::Relaxed);
    }
    // SAFETY: passes the event on unchanged.
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

/// Hooks for `duration` on this thread, pumping messages so the hooks run.
pub fn count(duration: Duration) -> Result<HookCounts, FeedError> {
    // SAFETY: global LL hooks with procedures in this module; hmod may be null for LL hooks.
    let kb = unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard), None, 0) }
        .map_err(|e| os_err("SetWindowsHookExW(WH_KEYBOARD_LL)", e))?;
    // SAFETY: as above.
    let ms = unsafe { SetWindowsHookExW(WH_MOUSE_LL, Some(mouse), None, 0) }
        .map_err(|e| os_err("SetWindowsHookExW(WH_MOUSE_LL)", e))?;
    let end = Instant::now() + duration;
    let mut msg = MSG::default();
    while Instant::now() < end {
        // SAFETY: valid MSG out pointer; any window of this thread.
        while unsafe { PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE) }.as_bool() {
            // SAFETY: message from PeekMessageW.
            unsafe { DispatchMessageW(&msg) };
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    // SAFETY: hooks installed above, removed once.
    unsafe {
        let _ = UnhookWindowsHookEx(kb);
        let _ = UnhookWindowsHookEx(ms);
    }
    Ok(HookCounts {
        key_clean: KEY_CLEAN.load(Ordering::Relaxed),
        key_injected: KEY_INJECTED.load(Ordering::Relaxed),
        mouse_clean: MOUSE_CLEAN.load(Ordering::Relaxed),
        mouse_injected: MOUSE_INJECTED.load(Ordering::Relaxed),
    })
}
