//! Low-level keyboard hook for the client stream window (spec 0021): while input is captured, keys
//! Windows would act on itself (Win, Alt+Tab, Alt+F4) go to the host instead. The hook runs on the
//! thread that installed it, inside that thread's message loop (`hook_thread` runs it in `GetMessageW`),
//! and must return fast: Windows skips a hook that takes too long.

use std::cell::Cell;
use std::marker::PhantomData;

use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, HC_ACTION, HHOOK, KBDLLHOOKSTRUCT, LLKHF_EXTENDED, LLKHF_INJECTED,
    SetWindowsHookExW, UnhookWindowsHookEx, WH_KEYBOARD_LL, WM_KEYDOWN, WM_SYSKEYDOWN,
};

/// One key as the hook sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawKey {
    /// Set-1 scan code; bit 0x200 marks AltGr's fake left Ctrl.
    pub scan: u16,
    pub extended: bool,
    pub vk: u16,
    pub down: bool,
}

/// What the hook calls with each key; `true` = swallow it.
pub type OnKey = fn(RawKey) -> bool;

/// What the hook saw since it was installed (diagnostics, spec 0021).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HookCounts {
    /// Key events the hook was called with.
    pub calls: u64,
    /// Injected by a program (`SendInput`): passed on without reaching `on_key`.
    pub injected: u64,
    /// Swallowed (`on_key` returned `true`).
    pub swallowed: u64,
}

thread_local! {
    // `const` init, no destructor: read from the hook without allocating.
    static ON_KEY: Cell<Option<OnKey>> = const { Cell::new(None) };
    static COUNTS: Cell<HookCounts> = const {
        Cell::new(HookCounts {
            calls: 0,
            injected: 0,
            swallowed: 0,
        })
    };
}

/// The installed hook; dropping it removes it. Not `Send`: it belongs to the installing thread.
#[derive(Debug)]
pub struct KeyHook {
    hook: HHOOK,
    _thread: PhantomData<*const ()>,
}

impl KeyHook {
    /// Hooks every key on this desktop; `on_key` returns `true` to swallow the key. Injected keys
    /// (`SendInput` from any program) pass without reaching `on_key`. The thread must pump messages.
    pub fn install(on_key: OnKey) -> windows::core::Result<KeyHook> {
        ON_KEY.with(|cell| cell.set(Some(on_key)));
        COUNTS.with(|cell| cell.set(HookCounts::default()));
        // SAFETY: `hook_proc` has the HOOKPROC signature; low-level hooks need no module or thread.
        let hook = unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook_proc), None, 0) };
        match hook {
            Ok(hook) => Ok(KeyHook {
                hook,
                _thread: PhantomData,
            }),
            Err(error) => {
                ON_KEY.with(|cell| cell.set(None));
                Err(error)
            }
        }
    }
}

impl KeyHook {
    /// Counts since `install` (read on the installing thread).
    pub fn counts(&self) -> HookCounts {
        COUNTS.with(Cell::get)
    }
}

impl Drop for KeyHook {
    fn drop(&mut self) {
        // SAFETY: the handle came from SetWindowsHookExW on this thread and is removed once.
        if let Err(error) = unsafe { UnhookWindowsHookEx(self.hook) } {
            log::warn!("keyboard hook not removed: {error}");
        }
        ON_KEY.with(|cell| cell.set(None));
    }
}

// HOT PATH
extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32
        && let Some(on_key) = ON_KEY.with(Cell::get)
    {
        // SAFETY: for HC_ACTION, `lparam` points to the KBDLLHOOKSTRUCT of this call.
        let info = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
        let injected = info.flags.contains(LLKHF_INJECTED);
        count(|c| {
            c.calls += 1;
            c.injected += u64::from(injected);
        });
        if !injected {
            let msg = u32::try_from(wparam.0).unwrap_or(0);
            let key = RawKey {
                scan: u16::try_from(info.scanCode).unwrap_or(0),
                extended: info.flags.contains(LLKHF_EXTENDED),
                vk: u16::try_from(info.vkCode).unwrap_or(0),
                down: msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN,
            };
            if on_key(key) {
                count(|c| c.swallowed += 1);
                return LRESULT(1);
            }
        }
    }
    // SAFETY: passes the call on unchanged, as every hook must when it does not swallow it.
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

fn count(f: impl FnOnce(&mut HookCounts)) {
    COUNTS.with(|cell| {
        let mut c = cell.get();
        f(&mut c);
        cell.set(c);
    });
}

#[cfg(test)]
#[path = "kbd_hook_tests.rs"]
mod tests;
