use super::*;

#[test]
fn drivers_held_names_the_app() {
    assert!(drivers_held(Some("Steam")).contains("Steam"));
    assert!(drivers_held(Some("Other App")).contains("Other App"));
}

#[test]
fn drivers_held_without_name_mentions_steam() {
    assert!(drivers_held(None).contains("Steam"));
}

#[test]
fn input_try_countdown_names_seconds() {
    assert!(input_try_countdown(5).contains("5 s"));
}

#[test]
fn probe_summary_in_ms() {
    let text = probe_summary("keyboard", 200.0, 800.0, 7000.0, 10, 9, 1);
    assert_eq!(
        text,
        "keyboard: p50 0.20 ms, p99 0.80 ms, max 7.00 ms; sent 10, received 9, lost 1"
    );
}
