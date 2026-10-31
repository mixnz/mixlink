use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::{RAWINPUTDEVICE, RAWINPUTDEVICE_FLAGS, RegisterRawInputDevices};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, HWND_MESSAGE, RegisterClassW, WINDOW_EX_STYLE, WINDOW_STYLE,
    WNDCLASSW,
};
use windows::core::w;

use super::*;

extern "system" fn wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: windows::Win32::Foundation::WPARAM,
    lparam: windows::Win32::Foundation::LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    // SAFETY: default handling for a throwaway window.
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

/// As winit does it: mouse and keyboard raw input on a message-only window of this process.
fn register_both() {
    // SAFETY: a message-only window for this test thread and a valid device list.
    unsafe {
        let class = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            lpszClassName: w!("MixLinkRawKeyboardTest"),
            ..Default::default()
        };
        RegisterClassW(&class);
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("MixLinkRawKeyboardTest"),
            w!(""),
            WINDOW_STYLE(0),
            0,
            0,
            0,
            0,
            Some(HWND_MESSAGE),
            None,
            None,
            None,
        )
        .unwrap();
        let device = |usage| RAWINPUTDEVICE {
            usUsagePage: 1,
            usUsage: usage,
            dwFlags: RAWINPUTDEVICE_FLAGS(0),
            hwndTarget: hwnd,
        };
        RegisterRawInputDevices(&[device(2), device(6)], size_of::<RAWINPUTDEVICE>() as u32)
            .unwrap();
    }
}

#[test]
fn keyboard_removed_mouse_kept() {
    register_both();
    assert_eq!(registered_usages(), vec![2, 6]);
    stop_raw_keyboard().unwrap();
    assert_eq!(registered_usages(), vec![2]);
    // Nothing left to remove: still fine.
    stop_raw_keyboard().unwrap();
}
