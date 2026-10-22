//! Raw Input receiver for latency runs: a message-only window with `RIDEV_INPUTSINK`.

use std::mem::{offset_of, size_of};
use std::sync::mpsc::Sender;

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::{
    GetRawInputData, HRAWINPUT, MOUSE_MOVE_ABSOLUTE, RAWHID, RAWINPUT, RAWINPUTDEVICE,
    RAWINPUTHEADER, RID_INPUT, RIDEV_INPUTSINK, RIM_TYPEHID, RIM_TYPEKEYBOARD, RIM_TYPEMOUSE,
    RegisterRawInputDevices,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, HWND_MESSAGE, MSG,
    RI_KEY_BREAK, RegisterClassExW, WINDOW_EX_STYLE, WINDOW_STYLE, WM_INPUT, WNDCLASSEXW,
};
use windows::core::w;

use super::{os_err, qpc};
use crate::feed::FeedError;
use crate::seq::{
    DeviceFilter, Probe, Unwrapper, decode_abs, decode_key, decode_pad_lx, decode_rel,
};
use crate::stats::Samples;

/// Gamepad report: [ID 4][buttons 2][hat 1][LX 2]... -> LX at bytes 4..6.
const PAD_LX_OFFSET: usize = 4;

extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // SAFETY: forwards the message unchanged to the default procedure.
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

fn usage(probe: Probe) -> (u16, u16) {
    match probe {
        Probe::Kbd => (1, 6),
        Probe::Rel | Probe::Abs => (1, 2),
        Probe::Pad | Probe::Wgi => (1, 5),
    }
}

/// Runs on the calling thread until `WM_QUIT`. Sends its thread id once input is registered.
pub fn receive(
    probe: Probe,
    filter: DeviceFilter,
    n: usize,
    ready: Sender<u32>,
) -> Result<Samples, FeedError> {
    let hwnd = create_window()?;
    let (page, use_) = usage(probe);
    let device = RAWINPUTDEVICE {
        usUsagePage: page,
        usUsage: use_,
        dwFlags: RIDEV_INPUTSINK,
        hwndTarget: hwnd,
    };
    // SAFETY: one valid RAWINPUTDEVICE for a window owned by this thread.
    unsafe { RegisterRawInputDevices(&[device], size_of::<RAWINPUTDEVICE>() as u32) }
        .map_err(|e| os_err("RegisterRawInputDevices", e))?;
    // SAFETY: no arguments.
    let _ = ready.send(unsafe { GetCurrentThreadId() });

    let mut samples = Samples::with_capacity(n);
    let mut unwrap = Unwrapper::new(probe.period());
    let mut buf = [0u64; 128];
    let mut msg = MSG::default();
    // SAFETY: valid MSG out pointer; loop ends on WM_QUIT (0) or error (-1).
    while unsafe { GetMessageW(&mut msg, None, 0, 0) }.0 > 0 {
        if msg.message == WM_INPUT {
            // HOT PATH
            let t = qpc::now();
            if let Some(residue) = read(msg.lParam, probe, &filter, &mut buf)
                && let Some(i) = unwrap.next(residue)
            {
                samples.record_recv(i as usize, t);
            }
        }
        // SAFETY: message from GetMessageW; WM_INPUT needs DefWindowProc for cleanup.
        unsafe { DispatchMessageW(&msg) };
    }
    Ok(samples)
}

/// Ends `receive` on thread `thread_id`.
pub fn quit(thread_id: u32) {
    use windows::Win32::UI::WindowsAndMessaging::{PostThreadMessageW, WM_QUIT};
    // SAFETY: plain message post; fails harmlessly if the thread is gone.
    let _ = unsafe { PostThreadMessageW(thread_id, WM_QUIT, WPARAM(0), LPARAM(0)) };
}

fn create_window() -> Result<HWND, FeedError> {
    // SAFETY: null module name = this executable.
    let instance = unsafe { GetModuleHandleW(None) }.map_err(|e| os_err("GetModuleHandleW", e))?;
    let class = WNDCLASSEXW {
        cbSize: size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(wndproc),
        hInstance: instance.into(),
        lpszClassName: w!("MixLinkS3Receiver"),
        ..Default::default()
    };
    // SAFETY: class struct is fully initialised; re-registration just fails harmlessly.
    unsafe { RegisterClassExW(&class) };
    // SAFETY: message-only window of the class registered above.
    unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("MixLinkS3Receiver"),
            w!("s3"),
            WINDOW_STYLE(0),
            0,
            0,
            0,
            0,
            Some(HWND_MESSAGE),
            None,
            Some(instance.into()),
            None,
        )
    }
    .map_err(|e| os_err("CreateWindowExW", e))
}

/// Decodes one WM_INPUT into a sequence residue, or None when it is not ours. HOT PATH
fn read(lparam: LPARAM, probe: Probe, filter: &DeviceFilter, buf: &mut [u64; 128]) -> Option<u32> {
    let mut size = size_of::<[u64; 128]>() as u32;
    // SAFETY: `buf` is 1 KiB, 8-byte aligned, and `size` says so.
    let got = unsafe {
        GetRawInputData(
            HRAWINPUT(lparam.0 as _),
            RID_INPUT,
            Some(buf.as_mut_ptr().cast()),
            &mut size,
            size_of::<RAWINPUTHEADER>() as u32,
        )
    };
    if got == u32::MAX || (got as usize) < size_of::<RAWINPUTHEADER>() {
        return None;
    }
    // SAFETY: GetRawInputData wrote a RAWINPUT into the aligned buffer.
    let raw = unsafe { &*(buf.as_ptr() as *const RAWINPUT) };
    if !filter.accepts(raw.header.hDevice.0 as isize) {
        return None;
    }
    match (probe, raw.header.dwType) {
        (Probe::Kbd, t) if t == RIM_TYPEKEYBOARD.0 => {
            // SAFETY: dwType says keyboard.
            let kb = unsafe { raw.data.keyboard };
            decode_key(kb.VKey, u32::from(kb.Flags) & RI_KEY_BREAK != 0)
        }
        (Probe::Rel | Probe::Abs, t) if t == RIM_TYPEMOUSE.0 => {
            // SAFETY: dwType says mouse.
            let m = unsafe { raw.data.mouse };
            let absolute = m.usFlags.0 & MOUSE_MOVE_ABSOLUTE.0 != 0;
            match probe {
                Probe::Rel if !absolute => decode_rel(m.lLastX),
                Probe::Abs if absolute => decode_abs(m.lLastX),
                _ => None,
            }
        }
        (Probe::Pad, t) if t == RIM_TYPEHID.0 => {
            let start = offset_of!(RAWINPUT, data) + offset_of!(RAWHID, bRawData);
            // SAFETY: the buffer holds `got` bytes written by GetRawInputData.
            let bytes =
                unsafe { std::slice::from_raw_parts(buf.as_ptr() as *const u8, got as usize) };
            let lx = bytes.get(start + PAD_LX_OFFSET..start + PAD_LX_OFFSET + 2)?;
            decode_pad_lx(u16::from_le_bytes([lx[0], lx[1]]))
        }
        _ => None,
    }
}
