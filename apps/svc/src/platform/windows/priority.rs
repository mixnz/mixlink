//! Priority levers of the capture / encode thread (spec 0007 Verdict, spec 0019).

use mixlink_core::mmcss::{Mmcss, MmcssTask};
use windows::Win32::Graphics::Direct3D11::ID3D11Device;
use windows::Win32::Graphics::Dxgi::IDXGIDevice;
use windows::Win32::System::Threading::{
    GetCurrentProcess, HIGH_PRIORITY_CLASS, NORMAL_PRIORITY_CLASS, SetPriorityClass,
};
use windows::core::Interface;

use crate::capture_args::Levers;

/// Thread and process levers in force; dropping it (on the same thread) undoes them.
#[derive(Debug)]
pub struct ThreadLevers {
    _mmcss: Option<Mmcss>,
    class: bool,
}

/// MMCSS "Pro Audio" critical for the calling thread, `HIGH_PRIORITY_CLASS` for the process. Failures
/// are logged and the run goes on without that lever.
pub fn apply_thread(levers: Levers) -> ThreadLevers {
    let mmcss = if levers.mmcss {
        Mmcss::enter(MmcssTask::ProAudio)
    } else {
        None
    };
    let class = levers.class && {
        // SAFETY: pseudo handle of this process; changes only its priority class.
        let raised = unsafe { SetPriorityClass(GetCurrentProcess(), HIGH_PRIORITY_CLASS) };
        if let Err(error) = &raised {
            log::warn!("high priority class refused: {error}");
        }
        raised.is_ok()
    };
    ThreadLevers {
        _mmcss: mmcss,
        class,
    }
}

impl Drop for ThreadLevers {
    fn drop(&mut self) {
        if self.class {
            // SAFETY: as in `apply_thread`.
            let _ = unsafe { SetPriorityClass(GetCurrentProcess(), NORMAL_PRIORITY_CLASS) };
        }
    }
}

/// `SetGPUThreadPriority(7)` on the device the encoder uses (per device, so after each reopen).
pub fn raise_gpu(device: &ID3D11Device) {
    let raised = device
        .cast::<IDXGIDevice>()
        // SAFETY: live device; changes only its GPU scheduling priority.
        .and_then(|dxgi| unsafe { dxgi.SetGPUThreadPriority(7) });
    if let Err(error) = raised {
        log::warn!("GPU thread priority refused: {error}");
    }
}
