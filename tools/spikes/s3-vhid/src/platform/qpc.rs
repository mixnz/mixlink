//! QueryPerformanceCounter: one system-wide clock for every timestamp.

use windows::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};

// HOT PATH
pub fn now() -> i64 {
    let mut t = 0i64;
    // SAFETY: valid out pointer; cannot fail on Windows XP and later.
    let _ = unsafe { QueryPerformanceCounter(&mut t) };
    t
}

pub fn freq() -> i64 {
    let mut f = 0i64;
    // SAFETY: valid out pointer; cannot fail on Windows XP and later.
    let _ = unsafe { QueryPerformanceFrequency(&mut f) };
    f
}
