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

/// Raw `(residue, qpc)` arrivals recorded by a receiver; fixed capacity, never reallocates.
#[derive(Clone, Debug)]
pub struct Arrivals {
    items: Vec<(u32, i64)>,
    dropped: usize,
}

impl Arrivals {
    pub fn with_capacity(n: usize) -> Self {
        Self {
            items: Vec::with_capacity(n),
            dropped: 0,
        }
    }

    // HOT PATH
    pub fn push(&mut self, residue: u32, qpc: i64) {
        if self.items.len() < self.items.capacity() {
            self.items.push((residue, qpc));
        } else {
            self.dropped += 1;
        }
    }

    pub fn as_slice(&self) -> &[(u32, i64)] {
        &self.items
    }

    /// Arrivals not kept because the buffer was full.
    pub fn dropped(&self) -> usize {
        self.dropped
    }
}

/// Matches raw arrivals `(residue, qpc)` to send times after the run: each arrival belongs to
/// the latest event sent at or before it with that residue (`i mod period`), and indices only
/// move forward. Arrivals before any send (stale readings), duplicates and late repeats are
/// dropped, and a lost event does not shift later ones.
pub fn assign_arrivals(sender: Samples, period: u32, arrivals: &[(u32, i64)]) -> Samples {
    let n = sender.sent.len();
    let mut recv = vec![None; n];
    let mut next_min = 0usize;
    let p = period as usize;
    for &(residue, t) in arrivals {
        let r = residue as usize;
        // Events are sent in index order, so "sent at or before t" is a prefix.
        let sent_before = sender
            .sent
            .partition_point(|s| matches!(s, Some(x) if *x <= t));
        let Some(k) = sent_before.checked_sub(1) else {
            continue;
        };
        let back = (k % p + p - r % p) % p;
        let Some(i) = k.checked_sub(back) else {
            continue;
        };
        if r >= p || i < next_min {
            continue;
        }
        recv[i] = Some(t);
        next_min = i + 1;
    }
    Samples {
        sent: sender.sent,
        recv,
    }
}

/// Whether a run may lose events: the HID path must deliver every one (spec 0006 case D);
/// a polled state API (XInput, WGI) can skip a state between two reads (case E).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Loss {
    Forbidden,
    Allowed,
}

/// Something received, p99 within 1 ms, and no loss where `loss` forbids it.
pub fn passes(s: &Summary, loss: Loss) -> bool {
    let loss_ok = match loss {
        Loss::Forbidden => s.lost == 0,
        Loss::Allowed => true,
    };
    s.received > 0 && loss_ok && s.p99_us <= 1000.0
}

#[cfg(test)]
#[path = "stats_tests.rs"]
mod tests;
