//! COM for the calling thread, for as long as the guard lives.

use windows::Win32::Foundation::RPC_E_CHANGED_MODE;
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize};

use crate::error::SvcError;

#[derive(Debug)]
pub struct ComGuard {
    /// `false` when the thread already had COM in another mode: not ours to uninitialize.
    owned: bool,
}

pub fn init() -> Result<ComGuard, SvcError> {
    // SAFETY: plain per-thread initialization; balanced by `Drop` when it succeeded.
    let hr = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    if hr == RPC_E_CHANGED_MODE {
        return Ok(ComGuard { owned: false });
    }
    hr.ok().map_err(SvcError::win32("CoInitializeEx"))?;
    Ok(ComGuard { owned: true })
}

impl Drop for ComGuard {
    fn drop(&mut self) {
        if self.owned {
            // SAFETY: pairs with the successful `CoInitializeEx` on this thread.
            unsafe { CoUninitialize() };
        }
    }
}
