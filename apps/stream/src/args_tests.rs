use std::path::PathBuf;

use mixlink_codec::VideoCodec;

use super::{ArgsError, PlayArgs, parse};

fn run(args: &[&str]) -> Result<Option<PlayArgs>, ArgsError> {
    parse(args.iter().map(|s| (*s).to_owned()))
}

#[test]
fn no_args_is_no_play_mode() {
    assert_eq!(run(&[]).unwrap(), None);
}

#[test]
fn play_takes_defaults() {
    let play = run(&["--play", "a.h264"]).unwrap().unwrap();
    assert_eq!(
        play,
        PlayArgs {
            path: PathBuf::from("a.h264"),
            codec: VideoCodec::H264,
            fps: 120,
            looping: false,
            measure: false,
        }
    );
}

#[test]
fn all_flags_are_read() {
    let play = run(&["--fps", "60", "--loop", "--play", "b.HEVC", "--measure"])
        .unwrap()
        .unwrap();
    assert_eq!(
        play,
        PlayArgs {
            path: PathBuf::from("b.HEVC"),
            codec: VideoCodec::Hevc,
            fps: 60,
            looping: true,
            measure: true,
        }
    );
}

#[test]
fn bad_fps_is_rejected() {
    for fps in ["0", "241", "x", "-5"] {
        assert_eq!(
            run(&["--play", "a.h264", "--fps", fps]),
            Err(ArgsError::BadFps(fps.to_owned())),
            "{fps}"
        );
    }
}

#[test]
fn flag_without_value_is_rejected() {
    assert_eq!(run(&["--play"]), Err(ArgsError::MissingValue("--play")));
    assert_eq!(
        run(&["--play", "a.h264", "--fps"]),
        Err(ArgsError::MissingValue("--fps"))
    );
}

#[test]
fn unknown_extension_is_rejected() {
    assert_eq!(
        run(&["--play", "a.mp4"]),
        Err(ArgsError::UnknownExtension(PathBuf::from("a.mp4")))
    );
    assert_eq!(
        run(&["--play", "noext"]),
        Err(ArgsError::UnknownExtension(PathBuf::from("noext")))
    );
}

#[test]
fn unknown_flag_is_rejected() {
    assert_eq!(
        run(&["--bogus"]),
        Err(ArgsError::UnknownFlag("--bogus".to_owned()))
    );
}

#[test]
fn play_options_without_play_are_rejected() {
    assert_eq!(run(&["--fps", "60"]), Err(ArgsError::NoPlay));
}
