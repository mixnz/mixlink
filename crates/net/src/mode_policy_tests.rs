use mixlink_core::ids::DeviceId;
use mixlink_proto::control::RejectReason;

use super::*;

fn mode(width: u32, height: u32, refresh_mhz: u32) -> DisplayInfo {
    DisplayInfo {
        width,
        height,
        refresh_mhz,
        hdr: false,
        dpi: 96,
    }
}

fn policy(codec: CodecChoice) -> ModePolicy {
    ModePolicy {
        max_fps: 120,
        codec,
    }
}

fn hello(displays: Vec<DisplayInfo>, index: u8, codecs: Vec<CodecId>) -> ControlMessage {
    ControlMessage::Hello {
        proto: 1,
        device: DeviceId::from_bytes([1; 16]),
        name: "Laptop".to_owned(),
        displays,
        stream_display: index,
        codecs,
    }
}

#[test]
fn client_mode_kept() {
    let p = policy(CodecChoice::Auto);
    let wanted = mode(2560, 1440, 120_000);
    assert_eq!(p.choose_mode(&[wanted], 0), Some(wanted));
}

#[test]
fn ntsc_refresh_rounds_up_within_max() {
    let p = policy(CodecChoice::Auto);
    let wanted = mode(1920, 1080, 119_880);
    assert_eq!(p.choose_mode(&[wanted], 0), Some(wanted));
}

#[test]
fn odd_width_falls_back_to_largest_fitting() {
    let p = policy(CodecChoice::Auto);
    let displays = [
        mode(1367, 768, 60_000),
        mode(1280, 720, 60_000),
        mode(1920, 1080, 60_000),
    ];
    assert_eq!(p.choose_mode(&displays, 0), Some(mode(1920, 1080, 60_000)));
}

#[test]
fn tie_prefers_higher_refresh() {
    let p = policy(CodecChoice::Auto);
    let displays = [
        mode(5120, 2880, 60_000),
        mode(1920, 1080, 60_000),
        mode(1920, 1080, 120_000),
    ];
    assert_eq!(p.choose_mode(&displays, 0), Some(mode(1920, 1080, 120_000)));
}

#[test]
fn refresh_above_max_fps_is_capped() {
    // A laptop with one 165 Hz screen still streams, at the host's 120 fps limit.
    let p = policy(CodecChoice::Auto);
    assert_eq!(
        p.choose_mode(&[mode(2560, 1440, 165_000)], 0),
        Some(mode(2560, 1440, 120_000))
    );
    let displays = [mode(1920, 1080, 60_000), mode(2560, 1440, 144_000)];
    assert_eq!(p.choose_mode(&displays, 1), Some(mode(2560, 1440, 120_000)));
}

#[test]
fn above_4k_is_not_used() {
    let p = policy(CodecChoice::Auto);
    assert_eq!(p.choose_mode(&[mode(5120, 2880, 60_000)], 0), None);
}

#[test]
fn none_fits_rejects() {
    let p = policy(CodecChoice::Auto);
    let msg = p.welcome(
        &hello(vec![mode(5120, 2880, 60_000)], 0, vec![CodecId::H264]),
        SessionId::new(3),
    );
    assert_eq!(
        msg,
        ControlMessage::Reject {
            reason: RejectReason::BadDisplay
        }
    );
}

#[test]
fn welcome_carries_mode_and_codec() {
    let p = policy(CodecChoice::Auto);
    let msg = p.welcome(
        &hello(
            vec![mode(1920, 1080, 60_000), mode(2560, 1440, 120_000)],
            1,
            vec![CodecId::H264, CodecId::Hevc],
        ),
        SessionId::new(3),
    );
    assert_eq!(
        msg,
        ControlMessage::Welcome {
            session: SessionId::new(3),
            mode: mode(2560, 1440, 120_000),
            codec: CodecId::Hevc,
        }
    );
}

#[test]
fn not_a_hello_rejects() {
    let p = policy(CodecChoice::Auto);
    assert_eq!(
        p.welcome(&ControlMessage::PairDone, SessionId::new(1)),
        ControlMessage::Reject {
            reason: RejectReason::BadDisplay
        }
    );
}

#[test]
fn auto_prefers_hevc() {
    let p = policy(CodecChoice::Auto);
    assert_eq!(
        p.choose_codec(&[CodecId::H264, CodecId::Hevc]),
        CodecId::Hevc
    );
}

#[test]
fn auto_without_hevc_is_h264() {
    assert_eq!(
        policy(CodecChoice::Auto).choose_codec(&[CodecId::H264]),
        CodecId::H264
    );
}

#[test]
fn forced_hevc_not_offered_is_h264() {
    assert_eq!(
        policy(CodecChoice::Hevc).choose_codec(&[CodecId::H264]),
        CodecId::H264
    );
    assert_eq!(
        policy(CodecChoice::Hevc).choose_codec(&[CodecId::Hevc]),
        CodecId::Hevc
    );
}

#[test]
fn forced_h264_wins_over_hevc() {
    assert_eq!(
        policy(CodecChoice::H264).choose_codec(&[CodecId::H264, CodecId::Hevc]),
        CodecId::H264
    );
}
