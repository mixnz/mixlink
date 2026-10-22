//! Nearest-rank percentiles over latency samples.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    pub p50: u64,
    pub p99: u64,
    pub max: u64,
    pub count: usize,
}

/// Sorts `samples` in place.
pub fn percentiles(samples: &mut [u64]) -> Stats {
    if samples.is_empty() {
        return Stats::default();
    }
    samples.sort_unstable();
    let n = samples.len();
    let rank = |p: usize| samples[(p * n).div_ceil(100).max(1) - 1];
    Stats {
        p50: rank(50),
        p99: rank(99),
        max: samples[n - 1],
        count: n,
    }
}

#[cfg(test)]
#[path = "stats_tests.rs"]
mod tests;
