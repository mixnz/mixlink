//! Per-frame record, so lost or slow frames can be placed in time (`<stem>-frames.csv`).

/// One frame as the receiver or the DDA probe saw it. Times are QPC ticks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameRecord {
    /// When the frame arrived (fence event or AcquireNextFrame return).
    pub t_qpc: u64,
    /// Decoded pattern counter; `None` when undecodable or not read (slot overwritten).
    pub counter: Option<u32>,
    pub ok: bool,
    pub generation: u32,
    pub latency_qpc: u64,
    /// Harness cost: copy + Map of the counter cell.
    pub map_qpc: u64,
}

pub struct FrameLog {
    records: Vec<FrameRecord>,
    overflow: u64,
}

impl FrameLog {
    /// Allocates once; `push` never allocates.
    pub fn new(capacity: usize) -> Self {
        Self {
            records: Vec::with_capacity(capacity),
            overflow: 0,
        }
    }

    // HOT PATH
    pub fn push(&mut self, rec: FrameRecord) {
        if self.records.len() < self.records.capacity() {
            self.records.push(rec);
        } else {
            self.overflow += 1;
        }
    }

    pub fn records(&self) -> &[FrameRecord] {
        &self.records
    }

    pub fn overflow(&self) -> u64 {
        self.overflow
    }
}

pub const CSV_HEADER: &str = "t_qpc,counter,ok,generation,latency_us,map_us";

pub fn csv_line(r: &FrameRecord, qpc_freq: u64) -> String {
    let us = |t: u64| ((u128::from(t) * 1_000_000) / u128::from(qpc_freq.max(1))) as u64;
    let counter = r.counter.map(|c| c.to_string()).unwrap_or_default();
    format!(
        "{},{counter},{},{},{},{}",
        r.t_qpc,
        u8::from(r.ok),
        r.generation,
        us(r.latency_qpc),
        us(r.map_qpc)
    )
}

#[cfg(test)]
#[path = "framelog_tests.rs"]
mod tests;
