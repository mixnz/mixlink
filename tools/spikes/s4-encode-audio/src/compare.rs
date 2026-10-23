//! Pooling of round samples per side and the spec's "not slower" rule (spec 0007 → Comparison).

use std::fmt::Write as _;

use crate::stats::percentiles;
use crate::util::BoxResult;

/// Floor that keeps noise from failing a sub-2 ms result.
const FLOOR_US: u64 = 200;

/// Pooled p99 of side A (or C) ≤ max(1.1 × p99 of side B, p99 of B + 200 µs).
pub fn not_slower(p99_a_us: u64, p99_b_us: u64) -> bool {
    let relative = p99_b_us * 11 / 10;
    p99_a_us <= relative.max(p99_b_us + FLOOR_US)
}

/// One µs value per line; blank lines and `#` comments are skipped.
pub fn parse_samples_csv(text: &str) -> BoxResult<Vec<u64>> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| Ok(l.parse::<u64>()?))
        .collect()
}

/// `sides`: (side letter, rounds of samples). Pooled stats per side, per-round p99, then the rule
/// for every side against B.
pub fn pool(sides: &[(char, Vec<Vec<u64>>)]) -> String {
    let mut out = String::new();
    let mut pooled_p99 = Vec::new();
    for (side, rounds) in sides {
        let mut all: Vec<u64> = rounds.iter().flatten().copied().collect();
        let s = percentiles(&mut all);
        let per_round: Vec<String> = rounds
            .iter()
            .map(|r| percentiles(&mut r.clone()).p99.to_string())
            .collect();
        let _ = writeln!(
            out,
            "side {side}: rounds={} samples={} p50={} p99={} max={} us; round p99 [{}]",
            rounds.len(),
            s.count,
            s.p50,
            s.p99,
            s.max,
            per_round.join(", ")
        );
        pooled_p99.push((*side, s.p99));
    }
    let Some(&(_, b)) = pooled_p99.iter().find(|(side, _)| *side == 'B') else {
        out.push_str("rule: no side B to compare against\n");
        return out;
    };
    for &(side, p99) in pooled_p99.iter().filter(|(side, _)| *side != 'B') {
        let verdict = if not_slower(p99, b) { "PASS" } else { "FAIL" };
        let _ = writeln!(
            out,
            "rule {side} vs B: p99 {p99} <= max({}, {}) us -> {verdict}",
            b * 11 / 10,
            b + FLOOR_US
        );
    }
    out
}

#[cfg(test)]
#[path = "compare_tests.rs"]
mod tests;
