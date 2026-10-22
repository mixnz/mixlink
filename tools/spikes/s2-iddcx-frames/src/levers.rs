//! S2b latency levers (spec 0005). Bits match `drivers/vdisplay/Levers.h`.

use std::fmt::Write as _;

use windows::Win32::Foundation::HANDLE;
use windows::Win32::Graphics::Direct3D11::ID3D11Device5;
use windows::Win32::Graphics::Dxgi::IDXGIDevice;
use windows::Win32::System::Threading::{
    AVRT_PRIORITY_CRITICAL, AvRevertMmThreadCharacteristics, AvSetMmThreadCharacteristicsW,
    AvSetMmThreadPriority,
};
use windows::core::{Interface, w};

use crate::header::S2Header;

pub const L1_CPU: u32 = 0x1;
pub const L2_GPU: u32 = 0x2;
pub const L3_NO_COPY: u32 = 0x4;
pub const L4_D3D12: u32 = 0x8;

const ALL: [u32; 4] = [L1_CPU, L2_GPU, L3_NO_COPY, L4_D3D12];

/// `"none"` for 0, else `"L1+L3"` style, lowest lever first.
pub fn lever_names(mask: u32) -> String {
    let names: Vec<String> = ALL
        .iter()
        .enumerate()
        .filter(|(_, bit)| mask & **bit != 0)
        .map(|(i, _)| format!("L{}", i + 1))
        .collect();
    if names.is_empty() {
        "none".to_string()
    } else {
        names.join("+")
    }
}

/// One summary line: requested and active levers, plus the HRESULT of each lever that failed.
pub fn lever_report(h: &S2Header) -> String {
    let mut t = format!(
        "levers requested {}, active {}",
        lever_names(h.levers_requested),
        lever_names(h.levers_active)
    );
    for (i, hr) in h.lever_hr.iter().enumerate() {
        if *hr != 0 {
            let _ = write!(t, ", failed L{} hr 0x{:08X}", i + 1, *hr as u32);
        }
    }
    t
}

/// Receiver side of L1/L2, applied on the receiver thread for each generation. Reverts MMCSS on drop.
pub struct ReceiverLevers {
    mmcss: Option<HANDLE>,
    notes: Vec<String>,
}

impl ReceiverLevers {
    /// What was applied or failed, for the run summary.
    pub fn notes(&self) -> &[String] {
        &self.notes
    }
}

impl Drop for ReceiverLevers {
    fn drop(&mut self) {
        if let Some(handle) = self.mmcss.take() {
            // SAFETY: handle came from AvSetMmThreadCharacteristicsW on this thread.
            unsafe {
                let _ = AvRevertMmThreadCharacteristics(handle);
            }
        }
    }
}

/// Mirrors the driver's active levers: L1 → MMCSS "Pro Audio" at critical priority on the calling
/// thread, L2 → `SetGPUThreadPriority(7)` on `device`. Failures become notes; the run goes on.
pub fn apply_receiver_levers(active: u32, device: &ID3D11Device5) -> ReceiverLevers {
    let mut levers = ReceiverLevers {
        mmcss: None,
        notes: Vec::new(),
    };
    if active & L1_CPU != 0 {
        let mut task = 0u32;
        // SAFETY: static task name, valid out-pointer; the handle is reverted in Drop.
        match unsafe { AvSetMmThreadCharacteristicsW(w!("Pro Audio"), &mut task) } {
            Ok(handle) => {
                levers.mmcss = Some(handle);
                // SAFETY: handle is the live MMCSS registration above.
                let set = unsafe { AvSetMmThreadPriority(handle, AVRT_PRIORITY_CRITICAL) };
                levers.notes.push(match set {
                    Ok(()) => "receiver L1 applied".to_string(),
                    Err(e) => format!("receiver L1 priority failed: {e}"),
                });
            }
            Err(e) => levers.notes.push(format!("receiver L1 failed: {e}")),
        }
    }
    if active & L2_GPU != 0 {
        // SAFETY: plain COM calls on a live device.
        let set = device
            .cast::<IDXGIDevice>()
            .and_then(|dxgi| unsafe { dxgi.SetGPUThreadPriority(7) });
        levers.notes.push(match set {
            Ok(()) => "receiver L2 applied".to_string(),
            Err(e) => format!("receiver L2 failed: {e}"),
        });
    }
    levers
}

#[cfg(test)]
#[path = "levers_tests.rs"]
mod tests;
