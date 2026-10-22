//! Latency samples (QPC ticks) and nearest-rank summary.

/// Preallocated send / receive times indexed by sequence number.
#[derive(Clone, Debug)]
pub struct Samples {
    sent: Vec<Option<i64>>,
    recv: Vec<Option<i64>>,
}

impl Samples {
    pub fn with_capacity(n: usize) -> Self {
        Self {
            sent: vec![None; n],
            recv: vec![None; n],
        }
    }

    /// Send times from the sender thread, receive times from the receiver thread.
    pub fn merge(sender: Samples, receiver: Samples) -> Samples {
        Samples {
            sent: sender.sent,
            recv: receiver.recv,
        }
    }

    pub fn len(&self) -> usize {
        self.sent.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sent.is_empty()
    }

    // HOT PATH
    pub fn record_sent(&mut self, i: usize, qpc: i64) {
        if let Some(slot) = self.sent.get_mut(i) {
            *slot = Some(qpc);
        }
    }

    /// First arrival wins; out-of-range indices are ignored. HOT PATH
    pub fn record_recv(&mut self, i: usize, qpc: i64) {
        if let Some(slot) = self.recv.get_mut(i)
            && slot.is_none()
        {
            *slot = Some(qpc);
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Summary {
    pub p50_us: f64,
    pub p99_us: f64,
    pub max_us: f64,
    pub sent: usize,
    pub received: usize,
    pub lost: usize,
}

/// Nearest-rank percentile of an ascending slice.
pub fn nearest_rank(sorted: &[i64], pct: u32) -> Option<i64> {
    if sorted.is_empty() {
        return None;
    }
    let rank = (pct as usize * sorted.len()).div_ceil(100).max(1);
    sorted.get(rank - 1).copied()
}

pub fn summary(samples: &Samples, qpc_freq: i64) -> Summary {
    let sent = samples.sent.iter().filter(|s| s.is_some()).count();
    let mut lat: Vec<i64> = samples
        .sent
        .iter()
        .zip(&samples.recv)
        .filter_map(|(s, r)| Some((*r)? - (*s)?))
        .collect();
    lat.sort_unstable();
    let to_us = |ticks: Option<i64>| ticks.map_or(0.0, |t| t as f64 * 1e6 / qpc_freq as f64);
    Summary {
        p50_us: to_us(nearest_rank(&lat, 50)),
        p99_us: to_us(nearest_rank(&lat, 99)),
        max_us: to_us(lat.last().copied()),
        sent,
        received: lat.len(),
        lost: sent - lat.len(),
    }
}

#[cfg(test)]
#[path = "stats_tests.rs"]
mod tests;
