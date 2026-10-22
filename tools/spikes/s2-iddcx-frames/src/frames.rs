//! Per-frame bookkeeping shared by the IddCx receiver and the DDA probe.

/// Which time the latency of a frame is measured from (spec 0004 → Measurement).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefKind {
    Present,
    Acquire,
}

/// `PresentDisplayQPCTime`, unless it is later than the driver acquire time.
pub fn reference_qpc(present_qpc: u64, acquire_qpc: u64) -> (u64, RefKind) {
    if present_qpc > acquire_qpc {
        (acquire_qpc, RefKind::Acquire)
    } else {
        (present_qpc, RefKind::Present)
    }
}

/// Latency samples to preallocate: DWM may drive the virtual monitor faster than its mode
/// (seen: 160 fps from a 160 Hz primary on a 120 Hz virtual mode), so size for 250 fps.
pub fn sample_capacity(minutes: u64) -> usize {
    (minutes * 60 * 250) as usize
}

/// Missed vsyncs between two `(PresentCount, PresentRefreshCount)` samples one present apart.
/// Disjoint samples (first one, several presents apart, or a jump over 60 refreshes) count 0.
pub fn vsync_glitches(prev: (u32, u32), cur: (u32, u32)) -> u32 {
    let dp = cur.0.wrapping_sub(prev.0);
    let dr = cur.1.wrapping_sub(prev.1);
    if dp != 1 || dr > 60 {
        return 0;
    }
    dr.saturating_sub(1)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TrackerSummary {
    /// Frames with a new counter value (latency sampled).
    pub delivered: u64,
    /// Counter values never delivered (includes the ones whose slot was overwritten).
    pub lost: u64,
    /// Same counter as the previous frame (static desktop or app missed a vsync).
    pub duplicates: u64,
    pub undecodable: u64,
    pub overwritten: u64,
    /// Delivered frames whose latency did not fit the preallocated buffer.
    pub overflow: u64,
}

pub struct FrameTracker {
    latencies: Vec<u64>,
    last: Option<u32>,
    summary: TrackerSummary,
}

impl FrameTracker {
    /// Allocates once; `on_frame` never allocates.
    pub fn new(capacity: usize) -> Self {
        Self {
            latencies: Vec::with_capacity(capacity),
            last: None,
            summary: TrackerSummary::default(),
        }
    }

    // HOT PATH
    pub fn on_frame(&mut self, counter: Option<u32>, slot_seq_ok: bool, latency_qpc: u64) {
        if !slot_seq_ok {
            self.summary.overwritten += 1;
            return;
        }
        let Some(n) = counter else {
            self.summary.undecodable += 1;
            return;
        };
        match self.last {
            Some(last) if n == last => {
                self.summary.duplicates += 1;
                return;
            }
            Some(last) if n > last => self.summary.lost += u64::from(n - last - 1),
            _ => {}
        }
        self.last = Some(n);
        self.summary.delivered += 1;
        if self.latencies.len() < self.latencies.capacity() {
            self.latencies.push(latency_qpc);
        } else {
            self.summary.overflow += 1;
        }
    }

    pub fn summary(&self) -> TrackerSummary {
        self.summary
    }

    pub fn latencies_mut(&mut self) -> &mut [u64] {
        &mut self.latencies
    }
}

#[cfg(test)]
#[path = "frames_tests.rs"]
mod tests;
