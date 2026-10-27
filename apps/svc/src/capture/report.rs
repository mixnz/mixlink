//! `capture try` numbers: preallocated samples (nothing grows in the frame loop) and the summary.

/// Values recorded per frame (nanoseconds or bytes). Full → further values are only counted.
#[derive(Debug)]
pub struct Samples {
    values: Vec<u64>,
    overflow: u64,
}

impl Samples {
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            values: Vec::with_capacity(capacity),
            overflow: 0,
        }
    }

    // HOT PATH
    pub fn push(&mut self, value: u64) {
        if self.values.len() < self.values.capacity() {
            self.values.push(value);
        } else {
            self.overflow += 1;
        }
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.values.len()
    }

    #[cfg(test)]
    pub fn overflow(&self) -> u64 {
        self.overflow
    }

    /// Nearest-rank percentile (`p` in 0..=100); 0 without samples. Sorts a copy: report time only.
    pub fn percentile(&self, p: f64) -> u64 {
        if self.values.is_empty() {
            return 0;
        }
        let mut sorted = self.values.clone();
        sorted.sort_unstable();
        let n = sorted.len();
        // Ceil of p% of n, as an index; p is clamped so the cast stays in range.
        let rank = ((p.clamp(0.0, 100.0) / 100.0) * n as f64).ceil() as usize;
        sorted[rank.clamp(1, n) - 1]
    }

    pub fn max(&self) -> u64 {
        self.values.iter().copied().max().unwrap_or(0)
    }
}

/// Frames a run of `seconds` at `refresh_mhz` (60 Hz when unknown) can bring, plus 10 %.
pub fn expected_frames(seconds: u32, refresh_mhz: u32) -> usize {
    let hz = if refresh_mhz == 0 {
        60_000
    } else {
        refresh_mhz
    };
    let frames = u64::from(seconds) * u64::from(hz) / 1000;
    usize::try_from(frames + frames / 10).unwrap_or(usize::MAX)
}

/// Everything `capture try` reports.
#[derive(Debug)]
pub struct Stats {
    pub frames: u64,
    pub idrs: u64,
    pub reopens: u64,
    pub hold_timeouts: u32,
    pub ring_fallbacks: u32,
    pub dropped_aus: u64,
    /// DWM present → frame ready for the encoder (ns).
    pub capture: Samples,
    /// Encode call (ns).
    pub encode: Samples,
    /// Access unit size (bytes).
    pub au_bytes: Samples,
}

impl Stats {
    pub fn with_capacity(frames: usize) -> Self {
        Self {
            frames: 0,
            idrs: 0,
            reopens: 0,
            hold_timeouts: 0,
            ring_fallbacks: 0,
            dropped_aus: 0,
            capture: Samples::with_capacity(frames),
            encode: Samples::with_capacity(frames),
            au_bytes: Samples::with_capacity(frames),
        }
    }
}

#[cfg(test)]
#[path = "report_tests.rs"]
mod tests;
