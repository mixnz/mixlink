//! MMCSS "Pro Audio" at critical priority for send / recv threads (specs 0005, 0007).

use windows::Win32::Foundation::HANDLE;
use windows::Win32::System::Threading::{
    AVRT_PRIORITY_CRITICAL, AvRevertMmThreadCharacteristics, AvSetMmThreadCharacteristicsW,
    AvSetMmThreadPriority,
};
use windows::core::w;

/// Live MMCSS registration of the calling thread; reverted on drop (same thread).
#[derive(Debug)]
pub(crate) struct Mmcss(HANDLE);

impl Mmcss {
    /// Best effort: `None` (logged once) when MMCSS refuses.
    pub(crate) fn enter() -> Option<Mmcss> {
        let mut task = 0u32;
        // SAFETY: static task name, valid out-pointer; the handle is reverted in Drop.
        match unsafe { AvSetMmThreadCharacteristicsW(w!("Pro Audio"), &mut task) } {
            Ok(handle) => {
                // SAFETY: handle is the live MMCSS registration above.
                if let Err(e) = unsafe { AvSetMmThreadPriority(handle, AVRT_PRIORITY_CRITICAL) } {
                    log::warn!("MMCSS priority not applied: {e}");
                }
                Some(Mmcss(handle))
            }
            Err(e) => {
                log::warn!("MMCSS not applied: {e}");
                None
            }
        }
    }
}

impl Drop for Mmcss {
    fn drop(&mut self) {
        // SAFETY: handle came from AvSetMmThreadCharacteristicsW on this thread.
        if let Err(e) = unsafe { AvRevertMmThreadCharacteristics(self.0) } {
            log::warn!("MMCSS revert failed: {e}");
        }
    }
}
