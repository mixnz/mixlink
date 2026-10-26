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
