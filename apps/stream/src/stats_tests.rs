use std::time::Duration;

use mixlink_core::clock::{Ticks, ticks_per_sec};
use mixlink_core::ids::FrameId;
use mixlink_core::stage::{Stage, StageStamps};

use super::{FrameStats, SpanStats};
use crate::au_queue::AuInfo;

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

/// A network frame: first fragment at 1 ms, last at 1.2 ms, decode 1.5–3 ms, presented 3.5 ms.
fn network_frame() -> (StageStamps, AuInfo) {
    let mut s = StageStamps::new(FrameId::new(1));
    s.mark_at(Stage::Received, Ticks::from_raw(micros(1_000)));
    s.mark_at(Stage::DecodeStart, Ticks::from_raw(micros(1_500)));
    s.mark_at(Stage::DecodeEnd, Ticks::from_raw(micros(3_000)));
    s.mark_at(Stage::Presented, Ticks::from_raw(micros(3_500)));
    let info = AuInfo {
        first: Ticks::from_raw(micros(1_000)),
        last: Ticks::from_raw(micros(1_200)),
        host_us: [400, 2_800, 50],
        network_us: 300,
    };
    (s, info)
}

#[test]
fn summary_has_all_spans() {
    let mut stats = FrameStats::new(10);
    let (s, info) = network_frame();
    stats.record_with(&s, &info);
    stats.presented = 1;
    let summary = stats.latency_summary();
    let names: Vec<&str> = summary.spans.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "capture",
            "encode",
            "send",
            "network",
            "reassemble",
            "queue",
            "decode",
            "present",
            "total"
        ]
    );
    let p50 = |name: &str| {
        summary
            .spans
            .iter()
            .find(|s| s.name == name)
            .unwrap()
            .p50_us
    };
    assert_eq!(p50("reassemble"), 200);
    assert_eq!(p50("queue"), 300);
    assert_eq!(p50("decode"), 1_500);
    // Host 3250 + network 300 + client 2500.
    assert_eq!(p50("total"), 6_050);
    assert_eq!(summary.frames, 1);
}

#[test]
fn play_mode_has_no_host_spans() {
    let mut stats = FrameStats::new(10);
    let mut s = frame(1, 5);
    s.mark_at(Stage::Received, Ticks::from_raw(micros(900)));
    stats.record(&s);
    let names: Vec<String> = stats
        .latency_summary()
        .spans
        .into_iter()
        .map(|s| s.name)
        .collect();
    assert_eq!(names, ["queue", "decode"]);
}
