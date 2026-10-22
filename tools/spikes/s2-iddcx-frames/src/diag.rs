//! Diagnostic times per frame (spec 0004 → Measurement), reported in the timing summary.

use crate::header::S2Slot;
use crate::qpc;

/// Diagnostic times per frame (spec 0004 → Measurement), in QPC ticks.
#[derive(Default)]
pub struct Diag {
    n: u64,
    acq_minus_present_sum: i128,
    acq_minus_present_min: i64,
    acq_minus_present_max: i64,
    fence_minus_acq_sum: u128,
    fence_minus_acq_max: u64,
}

impl Diag {
    // HOT PATH
    pub fn add(&mut self, slot: &S2Slot, fence_qpc: u64) {
        let amp = slot.acquire_qpc as i64 - slot.present_qpc as i64;
        let fma = fence_qpc.saturating_sub(slot.acquire_qpc);
        if self.n == 0 {
            (self.acq_minus_present_min, self.acq_minus_present_max) = (amp, amp);
        }
        self.n += 1;
        self.acq_minus_present_sum += i128::from(amp);
        self.acq_minus_present_min = self.acq_minus_present_min.min(amp);
        self.acq_minus_present_max = self.acq_minus_present_max.max(amp);
        self.fence_minus_acq_sum += u128::from(fma);
        self.fence_minus_acq_max = self.fence_minus_acq_max.max(fma);
    }

    pub fn notes(&self) -> Vec<String> {
        let n = self.n.max(1);
        let us = |t: i64| (t as f64) * 1e6 / qpc::freq() as f64;
        vec![
            format!(
                "diag acquire-present avg {:.0} us, min {:.0} us, max {:.0} us",
                us((self.acq_minus_present_sum / i128::from(n)) as i64),
                us(self.acq_minus_present_min),
                us(self.acq_minus_present_max)
            ),
            format!(
                "diag fence-acquire avg {:.0} us, max {:.0} us",
                us((self.fence_minus_acq_sum / u128::from(n)) as i64),
                us(self.fence_minus_acq_max as i64)
            ),
        ]
    }
}
