//! By hand (spec 0021 run): does Raw Input on a window of this process stop this process's keyboard
//! hook while that window is in front? `RAW=none|mouse|keyboard|keyboard-nohotkeys|both`, then
//! `cargo test -p mixlink-input raw_window_and_hook -- --ignored --nocapture`, click the window titled
//! "MixLink probe", type a few keys, wait for the result (20 s).

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::Input::{
    RAWINPUTDEVICE, RAWINPUTDEVICE_FLAGS, RIDEV_NOHOTKEYS, RegisterRawInputDevices,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, MSG, PM_REMOVE, PeekMessageW,
    RegisterClassW, SW_SHOW, ShowWindow, TranslateMessage, WM_INPUT, WM_KEYDOWN, WNDCLASSW,
    WS_EX_TOPMOST, WS_OVERLAPPEDWINDOW,
};
use windows::core::{HSTRING, w};

use super::HookThread;

static RAW_INPUTS: AtomicU64 = AtomicU64::new(0);
static KEYDOWNS: AtomicU64 = AtomicU64::new(0);

extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_INPUT => {
            RAW_INPUTS.fetch_add(1, Ordering::Relaxed);
        }
        WM_KEYDOWN => {
            KEYDOWNS.fetch_add(1, Ordering::Relaxed);
        }
        _ => {}
    }
    // SAFETY: default handling (WM_INPUT needs it for its cleanup).
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

fn devices(raw: &str, hwnd: HWND) -> Vec<RAWINPUTDEVICE> {
    let device = |usage: u16, flags: RAWINPUTDEVICE_FLAGS| RAWINPUTDEVICE {
        usUsagePage: 1,
        usUsage: usage,
        dwFlags: flags,
        hwndTarget: hwnd,
    };
    let none = RAWINPUTDEVICE_FLAGS(0);
    match raw {
        "mouse" => vec![device(2, none)],
        "keyboard" => vec![device(6, none)],
        "keyboard-nohotkeys" => vec![device(6, RIDEV_NOHOTKEYS)],
        "both" | "both-drop" => vec![device(2, none), device(6, none)],
        _ => Vec::new(),
    }
}

#[test]
#[ignore = "needs a person typing"]
fn raw_window_and_hook() {
    let raw = std::env::var("RAW").unwrap_or_else(|_| "none".to_owned());
    let seen = Arc::new(AtomicU64::new(0));
    let counter = Arc::clone(&seen);
    let hook = HookThread::start(Box::new(move |key| {
        if key.down {
            counter.fetch_add(1, Ordering::Relaxed);
            eprintln!("hook: key {:#x} down", key.vk);
        }
        false
    }))
    .unwrap();
    // SAFETY: a plain window on this thread, raw input registered for it, pumped below.
    unsafe {
        let class = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            lpszClassName: w!("MixLinkRawProbe"),
            ..Default::default()
        };
        RegisterClassW(&class);
        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST,
            w!("MixLinkRawProbe"),
            &HSTRING::from(format!("MixLink probe {raw}")),
            WS_OVERLAPPEDWINDOW,
            200,
            200,
            500,
            300,
            None,
            None,
            None,
            None,
        )
        .unwrap();
        let _ = ShowWindow(hwnd, SW_SHOW);
        let list = devices(&raw, hwnd);
        if !list.is_empty() {
            RegisterRawInputDevices(&list, size_of::<RAWINPUTDEVICE>() as u32).unwrap();
        }
        if raw == "both-drop" {
            super::super::raw_keyboard::stop_raw_keyboard().unwrap();
        }
        eprintln!("raw input: {raw}; click the \"MixLink probe\" window and type for 20 s");
        let deadline = Instant::now() + Duration::from_secs(20);
        let mut msg = MSG::default();
        while Instant::now() < deadline {
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    let counts = hook.stop();
    eprintln!(
        "RESULT raw={raw}: hook key downs {}, window WM_KEYDOWN {}, WM_INPUT {}, hook {counts:?}",
        seen.load(Ordering::Relaxed),
        KEYDOWNS.load(Ordering::Relaxed),
        RAW_INPUTS.load(Ordering::Relaxed)
    );
}
