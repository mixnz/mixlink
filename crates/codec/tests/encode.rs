//! NVENC encoder on the test's own D3D11 textures (spec 0019). Skipped where NVENC is missing (CI).
#![cfg(windows)]

mod common;

use common::hw_device_with_decoder;
use common::pixels::{bgra_texture, has_idr, has_sps, read_nv12_pixel};
use mixlink_codec::{
    CodecError, Decoder, Encoder, EncoderConfig, VideoCodec, has_hardware_encoder,
};
use mixlink_core::video::ColorMatrix;
use windows::Win32::Graphics::Direct3D11::ID3D11Device;

const W: u32 = 256;
const H: u32 = 144;

fn config(codec: VideoCodec) -> EncoderConfig {
    EncoderConfig {
        codec,
        width: W,
        height: H,
        fps: 60,
        bitrate_kbps: 4000,
    }
}

/// Hardware device with NVENC and a decoder for `codec`, or a skip message.
fn device_or_skip(codec: VideoCodec) -> Option<ID3D11Device> {
    let device = hw_device_with_decoder(codec).filter(|d| has_hardware_encoder(d, codec));
    if device.is_none() {
        eprintln!("skip: no NVENC encoder and decoder for {codec:?}");
    }
    device
}

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn encodes_foreign_texture_zero_copy(codec: VideoCodec) -> TestResult {
    let Some(device) = device_or_skip(codec) else {
        return Ok(());
    };
    let texture = bgra_texture(&device, W, H, [30, 120, 220, 255])?;
    let mut encoder = Encoder::new(&device, &config(codec))?;
    for i in 0..10 {
        let au = encoder.encode(&texture, false)?;
        assert!(!au.is_empty(), "frame {i}: empty AU");
        if i == 0 {
            assert!(has_idr(codec, au), "first AU has no IDR");
            assert!(has_sps(codec, au), "first AU has no SPS");
        }
    }
    Ok(())
}

#[test]
fn encodes_foreign_texture_zero_copy_h264() {
    encodes_foreign_texture_zero_copy(VideoCodec::H264).unwrap();
}

#[test]
fn encodes_foreign_texture_zero_copy_hevc() {
    encodes_foreign_texture_zero_copy(VideoCodec::Hevc).unwrap();
}

#[test]
fn forced_idr_on_request() {
    let codec = VideoCodec::H264;
    let Some(device) = device_or_skip(codec) else {
        return;
    };
    let texture = bgra_texture(&device, W, H, [200, 50, 100, 255]).unwrap();
    let mut encoder = Encoder::new(&device, &config(codec)).expect("encoder");
    for i in 0..8 {
        let au = encoder.encode(&texture, i == 5).expect("encode");
        let idr = has_idr(codec, au);
        match i {
            0 | 5 => assert!(idr && has_sps(codec, au), "frame {i}: expected IDR + SPS"),
            _ => assert!(!idr, "frame {i}: unexpected IDR"),
        }
    }
}

/// NVENC converts BGRA with BT.601 (spec 0019), so the stream must say so for the client to pick it.
fn decodes_as_tagged_bt601(codec: VideoCodec) -> TestResult {
    let Some(device) = device_or_skip(codec) else {
        return Ok(());
    };
    let texture = bgra_texture(&device, W, H, [200, 50, 100, 255])?;
    let mut encoder = Encoder::new(&device, &config(codec))?;
    let mut decoder = Decoder::new(&device, codec)?;
    let mut decoded = None;
    for _ in 0..5 {
        let au = encoder.encode(&texture, false)?.to_vec();
        if let Some(frame) = decoder.decode(&au)? {
            decoded = Some((
                read_nv12_pixel(&device, &frame, W / 2, H / 2)?,
                frame.matrix(),
            ));
        }
    }
    let Some(((y, u, v), matrix)) = decoded else {
        panic!("{codec:?}: no decoded frame");
    };
    assert_eq!(
        matrix,
        ColorMatrix::Bt601,
        "{codec:?}: stream not tagged BT.601"
    );
    // BT.601 limited range of RGB (200, 50, 100); BT.709 would be (89, 135, 192).
    let near = |got: u8, want: u8| got.abs_diff(want) <= 4;
    assert!(
        near(y, 102) && near(u, 128) && near(v, 190),
        "{codec:?}: got YUV ({y}, {u}, {v}), want BT.601 (102, 128, 190)"
    );
    Ok(())
}

#[test]
fn decodes_as_tagged_bt601_h264() {
    decodes_as_tagged_bt601(VideoCodec::H264).unwrap();
}

#[test]
fn decodes_as_tagged_bt601_hevc() {
    decodes_as_tagged_bt601(VideoCodec::Hevc).unwrap();
}

#[test]
fn new_size_needs_new_encoder() {
    let codec = VideoCodec::H264;
    let Some(device) = device_or_skip(codec) else {
        return;
    };
    let texture = bgra_texture(&device, 320, 180, [0, 0, 0, 255]).unwrap();
    let mut encoder = Encoder::new(&device, &config(codec)).expect("encoder");
    assert!(matches!(
        encoder.encode(&texture, false),
        Err(CodecError::FrameSize { .. })
    ));
}
