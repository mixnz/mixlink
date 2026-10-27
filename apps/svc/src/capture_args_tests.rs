use std::path::PathBuf;

use mixlink_codec::VideoCodec;
use mixlink_core::config::Topology;

use super::*;
use crate::args::TryTopology;

const CLIENT: &str = "00112233445566778899aabbccddeeff";

fn parse(text: &str) -> Result<CaptureCmd, String> {
    let args: Vec<&str> = text.split_whitespace().collect();
    parse_capture(&args)
}

fn try_cmd(text: &str) -> CaptureTry {
    match parse(text) {
        Ok(CaptureCmd::Try(cmd)) => cmd,
        Err(e) => panic!("{text}: {e}"),
    }
}

#[test]
fn parses_minimal() {
    let cmd = try_cmd(&format!(
        "try --client {CLIENT} --mode 2560x1440@120 --out a.h264"
    ));
    assert_eq!(cmd.client.to_string(), CLIENT);
    assert_eq!((cmd.mode.width, cmd.mode.height), (2560, 1440));
    assert_eq!(cmd.mode.refresh_mhz, 120_000);
    assert_eq!(cmd.topology, TryTopology::Keep);
    assert_eq!(cmd.codec, VideoCodec::H264);
    assert_eq!(cmd.seconds, 10);
    assert_eq!(cmd.bitrate_kbps, 30_000);
    assert_eq!(cmd.out, PathBuf::from("a.h264"));
    assert_eq!(
        cmd.levers,
        Levers {
            mmcss: true,
            gpu: true,
            class: true
        }
    );
}

#[test]
fn parses_all_flags() {
    let cmd = try_cmd(&format!(
        "try --out b.hevc --codec hevc --seconds 30 --bitrate 50000 --topology virtual-only \
         --levers mmcss,class --mode 1920x1080@59.94 --client {CLIENT}"
    ));
    assert_eq!(cmd.codec, VideoCodec::Hevc);
    assert_eq!(cmd.seconds, 30);
    assert_eq!(cmd.bitrate_kbps, 50_000);
    assert_eq!(cmd.topology, TryTopology::Apply(Topology::VirtualOnly));
    assert_eq!(cmd.mode.refresh_mhz, 59_940);
    assert_eq!(
        cmd.levers,
        Levers {
            mmcss: true,
            gpu: false,
            class: true
        }
    );
}

#[test]
fn levers_none() {
    let cmd = try_cmd(&format!(
        "try --client {CLIENT} --mode 1920x1080@60 --out a.h264 --levers none"
    ));
    assert_eq!(
        cmd.levers,
        Levers {
            mmcss: false,
            gpu: false,
            class: false
        }
    );
}

#[test]
fn rejects_bad_codec() {
    let err = parse(&format!(
        "try --client {CLIENT} --mode 1920x1080@60 --out a.h264 --codec av1"
    ))
    .unwrap_err();
    assert!(err.contains("--codec"), "{err}");
}

#[test]
fn out_extension_must_match_codec() {
    let err = parse(&format!(
        "try --client {CLIENT} --mode 1920x1080@60 --codec hevc --out a.h264"
    ))
    .unwrap_err();
    assert!(err.contains("--out"), "{err}");
}

#[test]
fn rejects_missing_and_repeated_flags() {
    assert!(parse("try --mode 1920x1080@60 --out a.h264").is_err());
    assert!(
        parse(&format!(
            "try --client {CLIENT} --client {CLIENT} --mode 1920x1080@60 --out a.h264"
        ))
        .is_err()
    );
    assert!(
        parse(&format!(
            "try --client {CLIENT} --mode 1920x1080@60 --out a.h264 --seconds 0"
        ))
        .is_err()
    );
    assert!(
        parse(&format!(
            "try --client {CLIENT} --mode 1920x1080@60 --out a.h264 --levers fast"
        ))
        .is_err()
    );
    assert!(parse("status").is_err());
}
