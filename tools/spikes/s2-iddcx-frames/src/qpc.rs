//! QueryPerformanceCounter: one system-wide clock shared by WUDFHost, Session 0 and the console session.

use std::sync::OnceLock;

use windows::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};

// HOT PATH
pub fn now() -> u64 {
    let mut t = 0i64;
    // SAFETY: valid out-pointer; QPC cannot fail on Windows XP and later.
    let _ = unsafe { QueryPerformanceCounter(&mut t) };
    t as u64
}

pub fn freq() -> u64 {
    static FREQ: OnceLock<u64> = OnceLock::new();
    *FREQ.get_or_init(|| {
        let mut f = 0i64;
        // SAFETY: valid out-pointer.
        let _ = unsafe { QueryPerformanceFrequency(&mut f) };
        f.max(1) as u64
    })
}

pub fn to_us(ticks: u64) -> u64 {
    ((u128::from(ticks) * 1_000_000) / u128::from(freq())) as u64
}
