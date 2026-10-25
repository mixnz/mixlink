//! MMCSS registration of the calling thread (specs 0005, 0007, 0013).

use windows::Win32::Foundation::HANDLE;
use windows::Win32::System::Threading::{
    AVRT_PRIORITY, AVRT_PRIORITY_CRITICAL, AVRT_PRIORITY_HIGH, AvRevertMmThreadCharacteristics,
    AvSetMmThreadCharacteristicsW, AvSetMmThreadPriority,
};
use windows::core::{PCWSTR, w};

use crate::mmcss::MmcssTask;

fn class(task: MmcssTask) -> (PCWSTR, AVRT_PRIORITY) {
    match task {
        MmcssTask::ProAudio => (w!("Pro Audio"), AVRT_PRIORITY_CRITICAL),
        MmcssTask::Games => (w!("Games"), AVRT_PRIORITY_HIGH),
    }
}

/// Live MMCSS registration of the calling thread; reverted on drop (same thread).
#[derive(Debug)]
pub struct Mmcss(HANDLE);

impl Mmcss {
    /// Best effort: `None` (logged) when MMCSS refuses.
    pub fn enter(task: MmcssTask) -> Option<Mmcss> {
        let (name, priority) = class(task);
        let mut index = 0u32;
        // SAFETY: static task name, valid out-pointer; the handle is reverted in Drop.
        match unsafe { AvSetMmThreadCharacteristicsW(name, &mut index) } {
            Ok(handle) => {
                // SAFETY: handle is the live MMCSS registration above.
                if let Err(e) = unsafe { AvSetMmThreadPriority(handle, priority) } {
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
