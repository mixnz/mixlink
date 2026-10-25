use std::time::Duration;

use mixlink_core::clock::{Ticks, ticks_per_sec};
use mixlink_core::ids::FrameId;
use mixlink_core::stage::{Stage, StageStamps};

use super::{FrameStats, SpanStats};

fn micros(us: u64) -> u64 {
    us * ticks_per_sec() / 1_000_000
}

/// Frame whose decode span is `us` microseconds.
fn frame(id: u32, us: u64) -> StageStamps {
    let base = micros(1_000);
    let mut s = StageStamps::new(FrameId::new(id));
    s.mark_at(Stage::DecodeStart, Ticks::from_raw(base));
    s.mark_at(Stage::DecodeEnd, Ticks::from_raw(base + micros(us)));
    s
}

fn us(v: u64) -> Duration {
    Duration::from_micros(v)
}

#[test]
fn percentiles_of_known_spans() {
    let mut stats = FrameStats::new(1000);
    for i in 1..=100 {
        stats.record(&frame(i as u32, i));
    }
    assert_eq!(
        stats.span(Stage::DecodeStart, Stage::DecodeEnd),
        Some(SpanStats {
            count: 100,
            p50: us(50),
            p99: us(99),
            max: us(100)
        })
    );
}

#[test]
fn ring_keeps_only_the_last_capacity() {
    let mut stats = FrameStats::new(10);
    for i in 1..=100 {
        stats.record(&frame(i as u32, i));
    }
    let span = stats.span(Stage::DecodeStart, Stage::DecodeEnd).unwrap();
    assert_eq!(span.count, 10);
    assert_eq!(span.max, us(100));
    assert_eq!(span.p50, us(95));
}

#[test]
fn unmarked_stages_are_skipped() {
    let mut stats = FrameStats::new(10);
    stats.record(&frame(1, 5));
    stats.record(&StageStamps::new(FrameId::new(2)));
    let span = stats.span(Stage::DecodeStart, Stage::DecodeEnd).unwrap();
    assert_eq!(span.count, 1);
    assert_eq!(stats.span(Stage::Received, Stage::Presented), None);
}

#[test]
fn summary_names_spans_and_counts() {
    let mut stats = FrameStats::new(10);
    stats.record(&frame(1, 5));
    stats.decoded = 3;
    stats.dropped = 1;
    let text = stats.summary();
    assert!(text.contains("decode"), "{text}");
    assert!(text.contains("decoded 3"), "{text}");
    assert!(text.contains("dropped 1"), "{text}");
}
