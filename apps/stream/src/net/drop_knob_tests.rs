use super::*;

#[test]
fn zero_never_drops() {
    let mut knob = DropKnob::default();
    assert!((0..10_000).all(|_| !knob.should_drop(DropLane::Video)));
}

#[test]
fn hundred_always_drops() {
    let mut knob = DropKnob::new(10_000, 0);
    assert!((0..1000).all(|_| knob.should_drop(DropLane::Video)));
    assert!((0..1000).all(|_| !knob.should_drop(DropLane::Cursor)));
}

#[test]
fn five_percent_is_about_five() {
    let mut knob = DropKnob::new(0, 500);
    let dropped = (0..100_000)
        .filter(|_| knob.should_drop(DropLane::Cursor))
        .count();
    assert!((4000..6000).contains(&dropped), "{dropped}");
}

#[test]
fn parses_both_parts() {
    let knob = DropKnob::parse("video=1,cursor=5").unwrap();
    assert_eq!((knob.video_bp, knob.cursor_bp), (100, 500));
    let knob = DropKnob::parse("cursor=7").unwrap();
    assert_eq!((knob.video_bp, knob.cursor_bp), (0, 700));
}

#[test]
fn bad_text_is_error() {
    for text in ["video", "video=101", "audio=1", "video=x", "", "video=-1"] {
        assert!(
            matches!(DropKnob::parse(text), Err(ArgsError::BadDrop(_))),
            "{text}"
        );
    }
}

#[test]
fn tenth_of_a_percent_parses_and_drops_about_that() {
    let mut knob = DropKnob::parse("video=0.1").unwrap();
    assert_eq!(knob.video_bp, 10);
    let dropped = (0..200_000)
        .filter(|_| knob.should_drop(DropLane::Video))
        .count();
    assert!((120..280).contains(&dropped), "{dropped}");
}
