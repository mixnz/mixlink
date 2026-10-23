use super::*;

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

#[test]
fn parse_defaults_nvenc_bgra() {
    let run = parse(&strings(&["--case", "B", "encode"])).unwrap();
    let Job::Encode(e) = run.job else {
        panic!("expected encode job")
    };
    assert_eq!(
        (e.backend, e.codec, e.format),
        (Backend::Nvenc, Codec::H264, Format::Bgra)
    );
    assert_eq!(
        (e.width, e.height, e.fps, e.frames),
        (2560, 1440, 120, 10_000)
    );
    assert_eq!(e.bitrate, 30_000_000);
    assert_eq!(e.encoder_name(), "h264_nvenc");
    assert_eq!((run.case.as_str(), run.side, run.delay_s), ("B", 'A', 0));
    assert!(!run.console && !run.in_console);
}

#[test]
fn parse_qsv_defaults_nv12() {
    let args = strings(&[
        "--console",
        "--side",
        "B",
        "encode",
        "--backend",
        "qsv",
        "--codec",
        "hevc",
    ]);
    let run = parse(&args).unwrap();
    let Job::Encode(e) = run.job else {
        panic!("expected encode job")
    };
    assert_eq!(e.format, Format::Nv12);
    assert_eq!(e.encoder_name(), "hevc_qsv");
    assert_eq!(run.side, 'B');
    assert!(run.console);
}

#[test]
fn parse_size_fps_frames() {
    let args = strings(&[
        "encode",
        "--size",
        "1920x1080",
        "--fps",
        "60",
        "--frames",
        "1200",
    ]);
    let Job::Encode(e) = parse(&args).unwrap().job else {
        panic!("expected encode job")
    };
    assert_eq!((e.width, e.height, e.fps, e.frames), (1920, 1080, 60, 1200));
}

#[test]
fn parse_both_with_tone() {
    let args = strings(&["both", "--minutes", "1", "--tone", "--delay-seconds", "30"]);
    let run = parse(&args).unwrap();
    let Job::Both(_, a) = run.job else {
        panic!("expected both job")
    };
    assert_eq!(
        a,
        AudioArgs {
            minutes: 1,
            tone: true
        }
    );
    assert_eq!(run.delay_s, 30);
}

#[test]
fn parse_rejects_unknown_job() {
    assert!(parse(&strings(&["--case", "A", "decode"])).is_err());
}

#[test]
fn parse_rejects_bad_backend_and_side() {
    assert!(parse(&strings(&["encode", "--backend", "amf"])).is_err());
    assert!(parse(&strings(&["encode", "--side", "D"])).is_err());
    assert!(parse(&strings(&["encode", "--size", "1080p"])).is_err());
}

#[test]
fn parse_levers_default_and_set() {
    let Job::Encode(e) = parse(&strings(&["encode"])).unwrap().job else {
        panic!("expected encode job")
    };
    assert_eq!(e.levers, 0);
    let Job::Encode(e) = parse(&strings(&["encode", "--levers", "7"])).unwrap().job else {
        panic!("expected encode job")
    };
    assert_eq!(e.levers, 7);
}
