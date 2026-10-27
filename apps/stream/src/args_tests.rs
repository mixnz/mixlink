use std::path::PathBuf;

use mixlink_codec::VideoCodec;

use super::{ArgsError, ConnectArgs, Mode, PlayArgs, parse};
use crate::net::drop_knob::DropKnob;

fn mode(args: &[&str]) -> Result<Option<Mode>, ArgsError> {
    parse(args.iter().map(|s| (*s).to_owned()))
}

fn run(args: &[&str]) -> Result<Option<PlayArgs>, ArgsError> {
    mode(args).map(|m| match m {
        Some(Mode::Play(play)) => Some(play),
        Some(Mode::Connect(c)) => panic!("not play mode: {c:?}"),
        None => None,
    })
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

const HOST: &str = "00112233445566778899aabbccddeeff";

#[test]
fn connect_parses() {
    let Some(Mode::Connect(c)) = mode(&["--connect", HOST, "--addr", "192.168.1.5:47801"]).unwrap()
    else {
        panic!("not connect mode");
    };
    assert_eq!(
        c,
        ConnectArgs {
            host: HOST.parse().unwrap(),
            addr: "192.168.1.5:47801".parse().unwrap(),
            drop: DropKnob::default(),
        }
    );
}

#[test]
fn drop_parses() {
    let args = [
        "--connect",
        HOST,
        "--addr",
        "10.0.0.2:47801",
        "--drop",
        "video=1,cursor=5",
    ];
    let Some(Mode::Connect(c)) = mode(&args).unwrap() else {
        panic!("not connect mode");
    };
    assert_eq!((c.drop.video_pct, c.drop.cursor_pct), (1, 5));
}

#[test]
fn connect_errors() {
    assert_eq!(mode(&["--connect", HOST]), Err(ArgsError::NoAddr));
    assert_eq!(
        mode(&["--connect", "xyz", "--addr", "1.2.3.4:5"]),
        Err(ArgsError::BadHost("xyz".to_owned()))
    );
    assert_eq!(
        mode(&["--connect", HOST, "--addr", "host:5"]),
        Err(ArgsError::BadAddr("host:5".to_owned()))
    );
    assert_eq!(
        mode(&["--connect", HOST, "--play", "a.h264"]),
        Err(ArgsError::BothModes)
    );
    assert_eq!(mode(&["--addr", "1.2.3.4:5"]), Err(ArgsError::NoConnect));
    assert_eq!(
        mode(&["--connect", HOST, "--addr", "1.2.3.4:5", "--loop"]),
        Err(ArgsError::NoPlay)
    );
}

#[test]
fn play_still_parses() {
    assert!(matches!(
        mode(&["--play", "a.hevc"]),
        Ok(Some(Mode::Play(_)))
    ));
}
