//! Polling receivers for latency runs: XInput (ViGEm pad) and Windows.Gaming.Input.
//! Busy-poll on a dedicated thread; QPC on the first changed reading.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use windows::Gaming::Input::{GameControllerSwitchPosition, RawGameController};
use windows::Win32::UI::Input::XboxController::{XINPUT_STATE, XInputGetState};

use super::{os_err, qpc};
use crate::feed::FeedError;
use crate::seq::{decode_wgi_axis, decode_xinput_lx};
use crate::stats::Arrivals;

pub fn xinput(user_index: u32, n: usize, stop: Arc<AtomicBool>) -> Arrivals {
    let mut arrivals = Arrivals::with_capacity(n * 2 + 16);
    let mut last_packet = None;
    while !stop.load(Ordering::Relaxed) {
        // HOT PATH
        let mut state = XINPUT_STATE::default();
        // SAFETY: valid out pointer.
        if unsafe { XInputGetState(user_index, &mut state) } != 0 {
            continue;
        }
        if last_packet == Some(state.dwPacketNumber) {
            continue;
        }
        let t = qpc::now();
        last_packet = Some(state.dwPacketNumber);
        if let Some(residue) = decode_xinput_lx(state.Gamepad.sThumbLX) {
            arrivals.push(residue, t);
        }
    }
    arrivals
}

/// Our pad as a `RawGameController` (matched by vendor/product id), waiting up to 3 s.
pub fn find_wgi(vendor: u16, product: u16) -> Result<RawGameController, FeedError> {
    for _ in 0..30 {
        let list =
            RawGameController::RawGameControllers().map_err(|e| os_err("RawGameControllers", e))?;
        for i in 0..list.Size().map_err(|e| os_err("IVectorView::Size", e))? {
            let c = list.GetAt(i).map_err(|e| os_err("IVectorView::GetAt", e))?;
            if c.HardwareVendorId().ok() == Some(vendor)
                && c.HardwareProductId().ok() == Some(product)
            {
                return Ok(c);
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    Err(FeedError::Os(
        "RawGameController for VID_DEED&PID_FEED not found within 3 s".into(),
    ))
}

/// Axis 0 is X (LX) in the descriptor's order.
pub fn wgi(
    controller: RawGameController,
    n: usize,
    stop: Arc<AtomicBool>,
) -> Result<Arrivals, FeedError> {
    let count = |r: windows::core::Result<i32>| {
        r.map(|v| v.max(0) as usize)
            .map_err(|e| os_err("RawGameController counts", e))
    };
    let mut buttons = vec![false; count(controller.ButtonCount())?];
    let mut switches =
        vec![GameControllerSwitchPosition::default(); count(controller.SwitchCount())?];
    let mut axes = vec![0f64; count(controller.AxisCount())?.max(1)];
    let mut arrivals = Arrivals::with_capacity(n * 2 + 16);
    let mut last_ts = None;
    while !stop.load(Ordering::Relaxed) {
        // HOT PATH
        let Ok(ts) = controller.GetCurrentReading(&mut buttons, &mut switches, &mut axes) else {
            continue;
        };
        if last_ts == Some(ts) {
            continue;
        }
        let t = qpc::now();
        last_ts = Some(ts);
        if let Some(residue) = decode_wgi_axis(axes[0]) {
            arrivals.push(residue, t);
        }
    }
    Ok(arrivals)
}
