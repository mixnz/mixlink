use super::*;
use crate::frames::TrackerSummary;
use crate::stats::Stats;

fn text() -> String {
    let summary = TrackerSummary {
        delivered: 999,
        lost: 1,
        duplicates: 3,
        undecodable: 0,
        overwritten: 1,
        overflow: 0,
    };
    let stats = Stats {
        p50: 420,
        p99: 870,
        max: 2_100,
        count: 999,
    };
    summary_text(
        "A",
        "iddcx",
        &summary,
        &stats,
        [990, 9],
        &["adapter X".to_string()],
    )
}

#[test]
fn summary_reports_lost_percent() {
    assert!(text().contains("lost 1 / 1000 (0.100 %)"), "{}", text());
}

#[test]
fn summary_reports_latency_in_us() {
    assert!(
        text().contains("p50 420 us, p99 870 us, max 2100 us (999 samples)"),
        "{}",
        text()
    );
}

#[test]
fn summary_reports_reference_counts_and_notes() {
    let t = text();
    assert!(t.contains("reference present 990, acquire 9"), "{t}");
    assert!(t.contains("adapter X"), "{t}");
}

#[test]
fn lost_percent_with_no_frames_is_zero() {
    assert_eq!(lost_percent(0, 0), 0.0);
}
