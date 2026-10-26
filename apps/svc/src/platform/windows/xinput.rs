//! XInput receiver for `input probe --device pad` (spec 0016): the virtual Xbox pad is read the way
//! XInput games read it, through the inbox `xinputhid` filter. Busy-polls on its own thread.

use std::sync::atomic::{AtomicBool, Ordering};

use mixlink_core::clock;
use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::UI::Input::XboxController::{XINPUT_STATE, XInputGetState};

/// Polls XInput `slot` until `stop` is set. Returns `(sThumbLX, QPC)` on every new packet, at most
/// `capacity` of them.
pub fn poll_thumb_lx(slot: u8, capacity: usize, stop: &AtomicBool) -> Vec<(i16, i64)> {
    let mut arrivals = Vec::with_capacity(capacity);
    let mut last_packet = None;
    while !stop.load(Ordering::Relaxed) {
        let mut state = XINPUT_STATE::default();
        // SAFETY: `state` is a live local the call fills; the index is 0..=3.
        let got = unsafe { XInputGetState(u32::from(slot), &raw mut state) };
        if got != ERROR_SUCCESS.0 || last_packet == Some(state.dwPacketNumber) {
            std::hint::spin_loop();
            continue;
        }
        // HOT PATH
        let t = i64::try_from(clock::now().raw()).unwrap_or(i64::MAX);
        last_packet = Some(state.dwPacketNumber);
        if arrivals.len() < capacity {
            arrivals.push((state.Gamepad.sThumbLX, t));
        }
    }
    arrivals
}
