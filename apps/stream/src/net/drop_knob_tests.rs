use super::*;

#[test]
fn zero_never_drops() {
    let mut knob = DropKnob::default();
    assert!((0..10_000).all(|_| !knob.should_drop(DropLane::Video)));
}

#[test]
fn hundred_always_drops() {
    let mut knob = DropKnob::new(100, 0);
    assert!((0..1000).all(|_| knob.should_drop(DropLane::Video)));
    assert!((0..1000).all(|_| !knob.should_drop(DropLane::Cursor)));
}

#[test]
fn five_percent_is_about_five() {
    let mut knob = DropKnob::new(0, 5);
    let dropped = (0..100_000)
        .filter(|_| knob.should_drop(DropLane::Cursor))
        .count();
    assert!((4000..6000).contains(&dropped), "{dropped}");
}

#[test]
fn parses_both_parts() {
    let knob = DropKnob::parse("video=1,cursor=5").unwrap();
    assert_eq!((knob.video_pct, knob.cursor_pct), (1, 5));
    let knob = DropKnob::parse("cursor=7").unwrap();
    assert_eq!((knob.video_pct, knob.cursor_pct), (0, 7));
}

#[test]
fn bad_text_is_error() {
    for text in ["video", "video=101", "audio=1", "video=x", ""] {
        assert!(
            matches!(DropKnob::parse(text), Err(ArgsError::BadDrop(_))),
            "{text}"
        );
    }
}
