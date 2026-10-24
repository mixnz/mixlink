//! QueryPerformanceCounter: one system-wide clock shared by WUDFHost, Session 0 and the console session.

use std::sync::OnceLock;

use windows::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};

// HOT PATH
#[inline]
pub(crate) fn now_raw() -> u64 {
    let mut t = 0i64;
    // SAFETY: valid out-pointer to a local; QPC cannot fail on Windows XP and later.
    let _ = unsafe { QueryPerformanceCounter(&mut t) };
    t.max(0) as u64
}

pub(crate) fn ticks_per_sec() -> u64 {
    static FREQ: OnceLock<u64> = OnceLock::new();
    *FREQ.get_or_init(|| {
        let mut f = 0i64;
        // SAFETY: valid out-pointer to a local.
        let _ = unsafe { QueryPerformanceFrequency(&mut f) };
        f.max(1) as u64
    })
}
