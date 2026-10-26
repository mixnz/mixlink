//! Raw Input receiver for `input probe` (spec 0016): a message-only window with `RIDEV_INPUTSINK`,
//! QPC taken when `WM_INPUT` is read. Ported from the spec 0006 harness. Run elevated: UIPI hides
//! input from a normal process while an elevated window has focus.

use std::mem::{offset_of, size_of};
use std::sync::mpsc::Sender;

use mixlink_core::clock;
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
    PostThreadMessageW, RI_KEY_BREAK, RegisterClassExW, WINDOW_EX_STYLE, WINDOW_STYLE, WM_INPUT,
    WM_QUIT, WNDCLASSEXW,
};
use windows::core::w;

use crate::error::SvcError;
use crate::input::probe::{ProbeDevice, decode_abs, decode_key, decode_pad_lx, decode_rel};

/// Gamepad Raw Input bytes: [ID 4][buttons 2][hat 1][LX 2]... → LX at 4..6.
const PAD_LX_OFFSET: usize = 4;
const PAD_REPORT_ID: u8 = 4;

extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // SAFETY: forwards the message unchanged to the default procedure.
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

/// HID usage page and usage of `device`'s collection.
fn usage(device: ProbeDevice) -> (u16, u16) {
    match device {
        ProbeDevice::Kbd => (1, 6),
        ProbeDevice::Rel | ProbeDevice::Abs => (1, 2),
        ProbeDevice::Pad => (1, 5),
    }
}

/// Receives on the calling thread until `quit`; sends this thread's id once input is registered.
/// Returns `(sequence residue, QPC)` per decoded arrival, at most `capacity` of them.
pub fn receive(
    device: ProbeDevice,
    capacity: usize,
    ready: &Sender<u32>,
) -> Result<Vec<(u32, i64)>, SvcError> {
    let hwnd = create_window()?;
    let (page, use_) = usage(device);
    let entry = RAWINPUTDEVICE {
        usUsagePage: page,
        usUsage: use_,
        dwFlags: RIDEV_INPUTSINK,
        hwndTarget: hwnd,
    };
    let entry_size = u32::try_from(size_of::<RAWINPUTDEVICE>()).unwrap_or(0);
    // SAFETY: one valid RAWINPUTDEVICE for a window owned by this thread.
    unsafe { RegisterRawInputDevices(&[entry], entry_size) }
        .map_err(SvcError::win32("RegisterRawInputDevices"))?;
    // SAFETY: no arguments.
    let _ = ready.send(unsafe { GetCurrentThreadId() });

    let mut arrivals = Vec::with_capacity(capacity);
    let mut buf = [0u64; 128];
    let mut msg = MSG::default();
    // SAFETY: valid MSG out pointer; the loop ends on WM_QUIT (0) or an error (-1).
    while unsafe { GetMessageW(&mut msg, None, 0, 0) }.0 > 0 {
        if msg.message == WM_INPUT {
            // HOT PATH
            let t = i64::try_from(clock::now().raw()).unwrap_or(i64::MAX);
            if let Some(residue) = read(msg.lParam, device, &mut buf)
                && arrivals.len() < capacity
            {
                arrivals.push((residue, t));
            }
        }
        // SAFETY: message from GetMessageW; WM_INPUT needs DefWindowProc for its cleanup.
        unsafe { DispatchMessageW(&msg) };
    }
    Ok(arrivals)
}

/// Ends `receive` on thread `thread_id`.
pub fn quit(thread_id: u32) {
    // SAFETY: plain message post; fails harmlessly when the thread is gone.
    let _ = unsafe { PostThreadMessageW(thread_id, WM_QUIT, WPARAM(0), LPARAM(0)) };
}

fn create_window() -> Result<HWND, SvcError> {
    // SAFETY: null module name = this executable.
    let instance =
        unsafe { GetModuleHandleW(None) }.map_err(SvcError::win32("GetModuleHandleW"))?;
    let class = WNDCLASSEXW {
        cbSize: u32::try_from(size_of::<WNDCLASSEXW>()).unwrap_or(0),
        lpfnWndProc: Some(wndproc),
        hInstance: instance.into(),
        lpszClassName: w!("MixLinkInputProbe"),
        ..Default::default()
    };
    // SAFETY: the class struct is fully initialised; registering twice just fails harmlessly.
    unsafe { RegisterClassExW(&class) };
    // SAFETY: message-only window of the class registered above.
    unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("MixLinkInputProbe"),
            w!("input probe"),
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
    .map_err(SvcError::win32("CreateWindowExW"))
}

/// Decodes one `WM_INPUT` into a sequence residue, or `None` when it is not ours.
// HOT PATH
fn read(lparam: LPARAM, device: ProbeDevice, buf: &mut [u64; 128]) -> Option<u32> {
    let mut size = u32::try_from(size_of::<[u64; 128]>()).unwrap_or(0);
    let header = u32::try_from(size_of::<RAWINPUTHEADER>()).unwrap_or(0);
    // SAFETY: `buf` is 1 KiB, 8-byte aligned, and `size` says so.
    let got = unsafe {
        GetRawInputData(
            HRAWINPUT(lparam.0 as _),
            RID_INPUT,
            Some(buf.as_mut_ptr().cast()),
            &raw mut size,
            header,
        )
    };
    let got = usize::try_from(got)
        .ok()
        .filter(|&g| g >= size_of::<RAWINPUTHEADER>())?;
    // SAFETY: GetRawInputData wrote a RAWINPUT into the aligned buffer.
    let raw = unsafe { &*buf.as_ptr().cast::<RAWINPUT>() };
    match device {
        ProbeDevice::Kbd if raw.header.dwType == RIM_TYPEKEYBOARD.0 => {
            // SAFETY: dwType says keyboard.
            let kb = unsafe { raw.data.keyboard };
            decode_key(kb.VKey, u32::from(kb.Flags) & RI_KEY_BREAK != 0)
        }
        ProbeDevice::Rel | ProbeDevice::Abs if raw.header.dwType == RIM_TYPEMOUSE.0 => {
            // SAFETY: dwType says mouse.
            let m = unsafe { raw.data.mouse };
            let absolute = m.usFlags.0 & MOUSE_MOVE_ABSOLUTE.0 != 0;
            match device {
                ProbeDevice::Rel if !absolute => decode_rel(m.lLastX),
                ProbeDevice::Abs if absolute => decode_abs(m.lLastX),
                ProbeDevice::Kbd | ProbeDevice::Rel | ProbeDevice::Abs | ProbeDevice::Pad => None,
            }
        }
        ProbeDevice::Pad if raw.header.dwType == RIM_TYPEHID.0 => {
            let start = offset_of!(RAWINPUT, data) + offset_of!(RAWHID, bRawData);
            // SAFETY: the buffer holds `got` bytes written by GetRawInputData.
            let bytes = unsafe { std::slice::from_raw_parts(buf.as_ptr().cast::<u8>(), got) };
            if bytes.get(start) != Some(&PAD_REPORT_ID) {
                return None;
            }
            let lx = bytes.get(start + PAD_LX_OFFSET..start + PAD_LX_OFFSET + 2)?;
            decode_pad_lx(u16::from_le_bytes([lx[0], lx[1]]))
        }
        ProbeDevice::Kbd | ProbeDevice::Rel | ProbeDevice::Abs | ProbeDevice::Pad => None,
    }
}
