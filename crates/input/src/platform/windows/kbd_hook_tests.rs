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

mod probe {
    use std::mem::size_of;
    use std::sync::mpsc;
    use std::thread;
    use std::time::{Duration, Instant};

    use windows::Win32::UI::Input::KeyboardAndMouse::{
        INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_KEYUP, SendInput,
        VK_F24,
    };

    use super::*;
    use crate::platform::windows::thread_msg::Pump;

    fn pass(_: RawKey) -> bool {
        false
    }

    fn f24(flags: KEYBD_EVENT_FLAGS) -> INPUT {
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VK_F24,
                    wScan: 0,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
    }

    /// By hand: `cargo test -p mixlink-input hook_sees_injected -- --ignored --nocapture`. Injected
    /// keys reach a low-level hook too (flagged), so this shows whether the hook is called at all.
    #[test]
    #[ignore = "needs an interactive desktop"]
    fn hook_sees_injected() {
        let (ready_tx, ready_rx) = mpsc::channel();
        let waiter = thread::spawn(move || {
            let pump = Pump::new().unwrap();
            let hook = KeyHook::install(pass).unwrap();
            ready_tx.send(()).unwrap();
            let deadline = Instant::now() + Duration::from_millis(500);
            while Instant::now() < deadline {
                pump.wait(Some(deadline));
            }
            hook.counts()
        });
        ready_rx.recv().unwrap();
        thread::sleep(Duration::from_millis(50));
        let inputs = [f24(KEYBD_EVENT_FLAGS(0)), f24(KEYEVENTF_KEYUP)];
        // SAFETY: two valid keyboard INPUTs of the declared size.
        let sent = unsafe { SendInput(&inputs, size_of::<INPUT>() as i32) };
        let counts = waiter.join().unwrap();
        eprintln!("sent {sent}, hook counts {counts:?}");
        assert_eq!(counts.calls, 2);
    }

    thread_local! {
        /// Physical key presses while a window of `HOOK_PROBE_PID` was in front, and while another was.
        static SPLIT: std::cell::Cell<(u64, u64)> = const { std::cell::Cell::new((0, 0)) };
        static TARGET_PID: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
    }

    fn split_by_foreground(key: RawKey) -> bool {
        use windows::Win32::UI::WindowsAndMessaging::{
            GetForegroundWindow, GetWindowThreadProcessId,
        };
        eprintln!("raw key: {key:?}");
        if !key.down {
            return false;
        }
        let mut pid = 0u32;
        // SAFETY: plain queries; `pid` is a valid out pointer.
        unsafe { GetWindowThreadProcessId(GetForegroundWindow(), Some(&mut pid)) };
        let target = TARGET_PID.with(std::cell::Cell::get);
        SPLIT.with(|s| {
            let (front, other) = s.get();
            s.set(if pid == target {
                (front + 1, other)
            } else {
                (front, other + 1)
            });
        });
        false
    }

    /// By hand: `HOOK_PROBE_PID=<pid of mixlink-stream>`; counts key presses split by whether that
    /// process's window was in front.
    #[test]
    #[ignore = "needs a person typing"]
    fn hook_split_by_window() {
        let target: u32 = std::env::var("HOOK_PROBE_PID")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        TARGET_PID.with(|t| t.set(target));
        let secs = std::env::var("HOOK_PROBE_SECS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(60);
        let pump = Pump::new().unwrap();
        let hook = KeyHook::install(split_by_foreground).unwrap();
        eprintln!("hook on for {secs} s, watching pid {target}");
        let deadline = Instant::now() + Duration::from_secs(secs);
        let mut last = HookCounts::default();
        while Instant::now() < deadline {
            pump.wait(Some(
                (Instant::now() + Duration::from_secs(1)).min(deadline),
            ));
            let now = hook.counts();
            if now != last {
                last = now;
                eprintln!("all hook calls so far: {now:?}");
            }
        }
        let (front, other) = SPLIT.with(std::cell::Cell::get);
        eprintln!(
            "presses with pid {target} in front: {front}; with another window in front: {other}; all calls {:?}",
            hook.counts()
        );
    }

    /// By hand on the client PC: type for 10 s; prints what the hook saw (physical keys are not injected).
    #[test]
    #[ignore = "needs a person typing"]
    fn hook_counts_typed_keys() {
        let pump = Pump::new().unwrap();
        let hook = KeyHook::install(pass).unwrap();
        let secs = std::env::var("HOOK_PROBE_SECS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(10);
        eprintln!("hook on: type some keys for {secs} s");
        let deadline = Instant::now() + Duration::from_secs(secs);
        while Instant::now() < deadline {
            pump.wait(Some(deadline));
        }
        eprintln!("hook counts after {secs} s: {:?}", hook.counts());
    }

    /// As `stream-input` does it: hook installed after a wake, then waits with no deadline.
    #[test]
    #[ignore = "needs an interactive desktop"]
    fn hook_sees_injected_no_deadline() {
        let (ready_tx, ready_rx) = mpsc::channel();
        let waiter = thread::spawn(move || {
            let _mmcss = mixlink_core::mmcss::Mmcss::enter(mixlink_core::mmcss::MmcssTask::Games);
            let pump = Pump::new().unwrap();
            ready_tx.send(pump.waker()).unwrap();
            let mut hook = None;
            loop {
                match pump.wait(None) {
                    crate::platform::windows::thread_msg::Pumped::Quit => break,
                    _ => {
                        if hook.is_none() {
                            hook = Some(KeyHook::install(pass).unwrap());
                        }
                    }
                }
            }
            hook.map(|h| h.counts())
        });
        let waker = ready_rx.recv().unwrap();
        waker.wake();
        thread::sleep(Duration::from_millis(100));
        let inputs = [f24(KEYBD_EVENT_FLAGS(0)), f24(KEYEVENTF_KEYUP)];
        // SAFETY: two valid keyboard INPUTs of the declared size.
        let sent = unsafe { SendInput(&inputs, size_of::<INPUT>() as i32) };
        thread::sleep(Duration::from_millis(200));
        waker.quit();
        let counts = waiter.join().unwrap();
        eprintln!("sent {sent}, hook counts {counts:?}");
        assert_eq!(counts.map(|c| c.calls), Some(2));
    }
}

mod focus_probe {
    use std::mem::size_of;
    use std::sync::mpsc;
    use std::thread;
    use std::time::{Duration, Instant};

    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_KEYUP, SendInput,
        VK_F24,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DispatchMessageW, GetForegroundWindow, MSG, PM_REMOVE,
        PeekMessageW, RegisterClassW, SW_SHOW, SetForegroundWindow, ShowWindow, WINDOW_EX_STYLE,
        WNDCLASSW, WS_OVERLAPPEDWINDOW,
    };
    use windows::core::w;

    use super::*;
    use crate::platform::windows::thread_msg::Pump;

    extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        // SAFETY: default handling for a throwaway window.
        unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
    }

    fn pass(_: RawKey) -> bool {
        false
    }

    fn f24(flags: KEYBD_EVENT_FLAGS) -> INPUT {
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VK_F24,
                    wScan: 0,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
    }

    fn swallow(_: RawKey) -> bool {
        true
    }

    /// By hand: hook A (first, `a_swallows` or passing; `a_pumps` false = its thread never waits for
    /// messages), hook B (second, passes). Prints what each saw for two injected keys.
    fn two_hooks(a_on_key: OnKey, a_pumps: bool) {
        let (a_tx, a_rx) = mpsc::channel();
        let a = thread::spawn(move || {
            let pump = Pump::new().unwrap();
            let hook = KeyHook::install(a_on_key).unwrap();
            a_tx.send(()).unwrap();
            if a_pumps {
                let deadline = Instant::now() + Duration::from_millis(1500);
                while Instant::now() < deadline {
                    pump.wait(Some(deadline));
                }
            } else {
                thread::sleep(Duration::from_millis(1500));
            }
            hook.counts()
        });
        a_rx.recv().unwrap();
        let (b_tx, b_rx) = mpsc::channel();
        let b = thread::spawn(move || {
            let pump = Pump::new().unwrap();
            let hook = KeyHook::install(pass).unwrap();
            b_tx.send(()).unwrap();
            let deadline = Instant::now() + Duration::from_millis(1400);
            while Instant::now() < deadline {
                pump.wait(Some(deadline));
            }
            hook.counts()
        });
        b_rx.recv().unwrap();
        thread::sleep(Duration::from_millis(50));
        let inputs = [f24(KEYBD_EVENT_FLAGS(0)), f24(KEYEVENTF_KEYUP)];
        let started = Instant::now();
        // SAFETY: two valid keyboard INPUTs of the declared size.
        let sent = unsafe { SendInput(&inputs, size_of::<INPUT>() as i32) };
        let took = started.elapsed();
        eprintln!(
            "sent {sent} in {took:?}: A {:?}, B {:?}",
            a.join().unwrap(),
            b.join().unwrap()
        );
    }

    /// A (first, passes, pumps), B (newest, its thread never waits for messages): does A still see keys?
    #[test]
    #[ignore = "needs an interactive desktop"]
    fn newest_hook_hung() {
        let (a_tx, a_rx) = mpsc::channel();
        let a = thread::spawn(move || {
            let pump = Pump::new().unwrap();
            let hook = KeyHook::install(pass).unwrap();
            a_tx.send(()).unwrap();
            let deadline = Instant::now() + Duration::from_millis(2500);
            while Instant::now() < deadline {
                pump.wait(Some(deadline));
            }
            hook.counts()
        });
        a_rx.recv().unwrap();
        let (b_tx, b_rx) = mpsc::channel();
        let b = thread::spawn(move || {
            let _pump = Pump::new().unwrap();
            let hook = KeyHook::install(pass).unwrap();
            b_tx.send(()).unwrap();
            thread::sleep(Duration::from_millis(2400));
            hook.counts()
        });
        b_rx.recv().unwrap();
        thread::sleep(Duration::from_millis(50));
        let inputs = [f24(KEYBD_EVENT_FLAGS(0)), f24(KEYEVENTF_KEYUP)];
        let started = Instant::now();
        // SAFETY: two valid keyboard INPUTs of the declared size.
        let sent = unsafe { SendInput(&inputs, size_of::<INPUT>() as i32) };
        eprintln!(
            "sent {sent} in {:?}: first-installed {:?}, newest (hung) {:?}",
            started.elapsed(),
            a.join().unwrap(),
            b.join().unwrap()
        );
    }

    #[test]
    #[ignore = "needs an interactive desktop"]
    fn two_hooks_order() {
        two_hooks(swallow, true);
    }

    #[test]
    #[ignore = "needs an interactive desktop"]
    fn two_hooks_first_hung() {
        two_hooks(pass, false);
    }

    /// By hand: a window of this process in the foreground (its own UI thread), the hook on another
    /// thread, injected keys: does the hook still see them?
    #[test]
    #[ignore = "needs an interactive desktop"]
    fn hook_with_own_window_foreground() {
        let (hook_tx, hook_rx) = mpsc::channel();
        let hooker = thread::spawn(move || {
            let pump = Pump::new().unwrap();
            let hook = KeyHook::install(pass).unwrap();
            hook_tx.send(()).unwrap();
            let deadline = Instant::now() + Duration::from_millis(1500);
            while Instant::now() < deadline {
                pump.wait(Some(deadline));
            }
            hook.counts()
        });
        hook_rx.recv().unwrap();
        let ui = thread::spawn(|| {
            // SAFETY: plain window class and window for this test thread.
            unsafe {
                let class = WNDCLASSW {
                    lpfnWndProc: Some(wndproc),
                    lpszClassName: w!("MixLinkHookProbe"),
                    ..Default::default()
                };
                RegisterClassW(&class);
                let hwnd = CreateWindowExW(
                    WINDOW_EX_STYLE(0),
                    w!("MixLinkHookProbe"),
                    w!("probe"),
                    WS_OVERLAPPEDWINDOW,
                    100,
                    100,
                    300,
                    200,
                    None,
                    None,
                    None,
                    None,
                )
                .unwrap();
                let _ = ShowWindow(hwnd, SW_SHOW);
                let _ = SetForegroundWindow(hwnd);
                let mut msg = MSG::default();
                let deadline = Instant::now() + Duration::from_millis(300);
                while Instant::now() < deadline {
                    while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                        DispatchMessageW(&msg);
                    }
                    thread::sleep(Duration::from_millis(5));
                }
                let front = GetForegroundWindow() == hwnd;
                let inputs = [f24(KEYBD_EVENT_FLAGS(0)), f24(KEYEVENTF_KEYUP)];
                let sent = SendInput(&inputs, size_of::<INPUT>() as i32);
                let deadline = Instant::now() + Duration::from_millis(500);
                while Instant::now() < deadline {
                    while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                        DispatchMessageW(&msg);
                    }
                    thread::sleep(Duration::from_millis(5));
                }
                (front, sent)
            }
        });
        let (front, sent) = ui.join().unwrap();
        let counts = hooker.join().unwrap();
        eprintln!("own window foreground: {front}, sent {sent}, hook counts {counts:?}");
    }
}
