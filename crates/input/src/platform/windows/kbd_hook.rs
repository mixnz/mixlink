//! Low-level keyboard hook for the client stream window (spec 0021): while input is captured, keys
//! Windows would act on itself (Win, Alt+Tab, Alt+F4) go to the host instead. The hook runs on the
//! thread that installed it, inside that thread's message wait (`thread_msg::Pump`), and must return
//! fast: Windows skips a hook that takes too long.

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

thread_local! {
    // `const` init, no destructor: read from the hook without allocating.
    static ON_KEY: Cell<Option<OnKey>> = const { Cell::new(None) };
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
        if !info.flags.contains(LLKHF_INJECTED) {
            let msg = u32::try_from(wparam.0).unwrap_or(0);
            let key = RawKey {
                scan: u16::try_from(info.scanCode).unwrap_or(0),
                extended: info.flags.contains(LLKHF_EXTENDED),
                vk: u16::try_from(info.vkCode).unwrap_or(0),
                down: msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN,
            };
            if on_key(key) {
                return LRESULT(1);
            }
        }
    }
    // SAFETY: passes the call on unchanged, as every hook must when it does not swallow it.
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;
    use crate::platform::windows::thread_msg::Pump;

    fn swallow_nothing(_: RawKey) -> bool {
        false
    }

    /// By hand on an interactive desktop: `cargo test -p mixlink-input hook_installs -- --ignored`.
    /// Installs, pumps briefly (type a key: it still reaches the focused window), removes.
    #[test]
    #[ignore = "needs an interactive desktop"]
    fn hook_installs_and_removes() {
        let pump = Pump::new().unwrap();
        let hook = KeyHook::install(swallow_nothing).unwrap();
        pump.wait(Some(Instant::now() + Duration::from_millis(200)));
        drop(hook);
        assert!(ON_KEY.with(Cell::get).is_none());
    }
}
