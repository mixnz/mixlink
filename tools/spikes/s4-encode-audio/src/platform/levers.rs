//! Encode priority levers (like S2b's): bit 1 = MMCSS "Pro Audio" at critical priority on the encode
//! thread, bit 2 = `SetGPUThreadPriority(7)` on the encode device, bit 4 = high process priority class.

use windows::Win32::Foundation::HANDLE;
use windows::Win32::Graphics::Direct3D11::ID3D11Device;
use windows::Win32::Graphics::Dxgi::IDXGIDevice;
use windows::Win32::System::Threading::{
    AVRT_PRIORITY_CRITICAL, AvRevertMmThreadCharacteristics, AvSetMmThreadCharacteristicsW,
    AvSetMmThreadPriority, GetCurrentProcess, HIGH_PRIORITY_CLASS, SetPriorityClass,
};
use windows::core::{Interface, w};

pub const L_MMCSS: u32 = 1;
pub const L_GPU: u32 = 2;
pub const L_PROCESS: u32 = 4;

/// `"mmcss+gpu"` style names for a mask; `"none"` for 0.
pub fn lever_names(mask: u32) -> String {
    let names: Vec<&str> = [(L_MMCSS, "mmcss"), (L_GPU, "gpu"), (L_PROCESS, "process")]
        .iter()
        .filter(|(bit, _)| mask & bit != 0)
        .map(|(_, name)| *name)
        .collect();
    if names.is_empty() {
        "none".into()
    } else {
        names.join("+")
    }
}

/// Applied levers; reverts MMCSS on drop. `notes` says what was applied or failed.
pub struct Levers {
    mmcss: Option<HANDLE>,
    pub notes: Vec<String>,
}

impl Drop for Levers {
    fn drop(&mut self) {
        if let Some(handle) = self.mmcss.take() {
            // SAFETY: handle came from AvSetMmThreadCharacteristicsW on this thread.
            unsafe {
                let _ = AvRevertMmThreadCharacteristics(handle);
            }
        }
    }
}

/// Applies `mask` on the calling (encode) thread and `device`. Failures become notes; the run goes on.
pub fn apply(mask: u32, device: &ID3D11Device) -> Levers {
    let mut levers = Levers {
        mmcss: None,
        notes: Vec::new(),
    };
    if mask & L_MMCSS != 0 {
        let mut task = 0u32;
        // SAFETY: static task name, valid out-pointer; the handle is reverted in Drop.
        match unsafe { AvSetMmThreadCharacteristicsW(w!("Pro Audio"), &mut task) } {
            Ok(handle) => {
                levers.mmcss = Some(handle);
                // SAFETY: handle is the live MMCSS registration above.
                let set = unsafe { AvSetMmThreadPriority(handle, AVRT_PRIORITY_CRITICAL) };
                levers.notes.push(match set {
                    Ok(()) => "mmcss applied".into(),
                    Err(e) => format!("mmcss priority failed: {e}"),
                });
            }
            Err(e) => levers.notes.push(format!("mmcss failed: {e}")),
        }
    }
    if mask & L_GPU != 0 {
        // SAFETY: plain COM calls on a live device.
        let set = device
            .cast::<IDXGIDevice>()
            .and_then(|dxgi| unsafe { dxgi.SetGPUThreadPriority(7) });
        levers.notes.push(match set {
            Ok(()) => "gpu priority 7 applied".into(),
            Err(e) => format!("gpu priority failed: {e}"),
        });
    }
    if mask & L_PROCESS != 0 {
        // SAFETY: pseudo-handle of this process.
        let set = unsafe { SetPriorityClass(GetCurrentProcess(), HIGH_PRIORITY_CLASS) };
        levers.notes.push(match set {
            Ok(()) => "high priority class applied".into(),
            Err(e) => format!("priority class failed: {e}"),
        });
    }
    levers
}

#[cfg(test)]
#[path = "levers_tests.rs"]
mod tests;
