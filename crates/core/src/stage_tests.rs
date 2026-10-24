use std::time::Duration;

use super::*;
use crate::clock::{Ticks, now, ticks_per_sec};
use crate::ids::FrameId;

#[test]
fn all_matches_index() {
    assert_eq!(Stage::ALL.len(), Stage::COUNT);
    for (i, stage) in Stage::ALL.iter().enumerate() {
        assert_eq!(stage.index(), i);
    }
}

#[test]
fn unmarked_stage_is_none() {
    let stamps = StageStamps::new(FrameId::new(3));
    assert_eq!(stamps.get(Stage::Sent), None);
    assert_eq!(stamps.frame(), FrameId::new(3));
}

#[test]
fn span_is_difference() {
    let f = ticks_per_sec();
    let mut stamps = StageStamps::new(FrameId::new(0));
    stamps.mark_at(Stage::EncodeStart, Ticks::from_raw(100));
    stamps.mark_at(Stage::EncodeEnd, Ticks::from_raw(100 + f));
    assert_eq!(
        stamps.span(Stage::EncodeStart, Stage::EncodeEnd),
        Some(Duration::from_secs(1))
    );
}

#[test]
fn span_with_unmarked_is_none() {
    let mut stamps = StageStamps::new(FrameId::new(0));
    stamps.mark_at(Stage::Captured, Ticks::from_raw(100));
    assert_eq!(stamps.span(Stage::Captured, Stage::Sent), None);
}

#[test]
fn span_backwards_is_none() {
    let mut stamps = StageStamps::new(FrameId::new(0));
    stamps.mark_at(Stage::DecodeStart, Ticks::from_raw(200));
    stamps.mark_at(Stage::DecodeEnd, Ticks::from_raw(100));
    assert_eq!(stamps.span(Stage::DecodeStart, Stage::DecodeEnd), None);
}

#[test]
fn mark_uses_clock() {
    let mut stamps = StageStamps::new(FrameId::new(0));
    let before = now();
    stamps.mark(Stage::Captured);
    let after = now();
    let marked = stamps.get(Stage::Captured).unwrap();
    assert!(before <= marked && marked <= after);
}
