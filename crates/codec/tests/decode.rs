//! D3D11VA decoder tests. Hardware tests skip when the GPU has no decoder (CI).

mod common;

use common::{access_units, device_or_skip, warp_device};
use mixlink_codec::{CodecError, DecodedFrame, Decoder, EXTRA_SURFACES, VideoCodec};
use mixlink_core::video::ColorMatrix;
use windows::Win32::Graphics::Direct3D11::{D3D11_BIND_SHADER_RESOURCE, D3D11_TEXTURE2D_DESC};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_NV12;

fn desc(frame: &DecodedFrame) -> D3D11_TEXTURE2D_DESC {
    let mut desc = D3D11_TEXTURE2D_DESC::default();
    // SAFETY: the texture is alive while the frame is held; `desc` is a valid out-pointer.
    unsafe { frame.texture().GetDesc(&mut desc) };
    desc
}

#[test]
fn warp_device_has_no_decoder() {
    let result = Decoder::new(&warp_device().unwrap(), VideoCodec::H264);
    assert!(matches!(
        result,
        Err(CodecError::NoHardwareDecoder(VideoCodec::H264))
    ));
}

fn decodes_all_frames(codec: VideoCodec, gpu_copy: bool) -> Result<(), CodecError> {
    let Some(device) = device_or_skip(codec) else {
        return Ok(());
    };
    let mut decoder = if gpu_copy {
        Decoder::with_gpu_copy(&device, codec)?
    } else {
        Decoder::new(&device, codec)?
    };
    for (i, au) in access_units(codec)?.iter().enumerate() {
        let Some(frame) = decoder.decode(au)? else {
            panic!("AU {i}: no frame");
        };
        assert_eq!((frame.width(), frame.height()), (256, 144));
        let desc = desc(&frame);
        assert_eq!(desc.Format, DXGI_FORMAT_NV12);
        assert!(frame.array_index() < desc.ArraySize);
        if decoder.uses_gpu_copy() {
            assert_eq!(frame.array_index(), 0);
        }
        // Either the decoder pool or the copy target must be sampleable by the presenter.
        assert_ne!(
            desc.BindFlags & D3D11_BIND_SHADER_RESOURCE.0 as u32,
            0,
            "AU {i}"
        );
    }
    if gpu_copy {
        assert!(decoder.uses_gpu_copy());
    }
    Ok(())
}

#[test]
fn test_stream_reports_bt709() {
    let codec = VideoCodec::H264;
    let Some(device) = device_or_skip(codec) else {
        return;
    };
    let mut decoder = Decoder::new(&device, codec).unwrap();
    let frame = access_units(codec)
        .unwrap()
        .iter()
        .find_map(|au| decoder.decode(au).unwrap())
        .expect("a decoded frame");
    assert_eq!(frame.matrix(), ColorMatrix::Bt709);
}

#[test]
fn decodes_tiny_h264_all_frames() {
    decodes_all_frames(VideoCodec::H264, false).unwrap();
}

#[test]
fn decodes_tiny_hevc_all_frames() {
    decodes_all_frames(VideoCodec::Hevc, false).unwrap();
}

#[test]
fn gpu_copy_path_decodes_all_frames() {
    decodes_all_frames(VideoCodec::H264, true).unwrap();
}

#[test]
fn corrupt_au_recovers_at_next_idr() {
    let codec = VideoCodec::H264;
    let Some(device) = device_or_skip(codec) else {
        return;
    };
    let mut decoder = Decoder::new(&device, codec).unwrap();
    let mut aus = access_units(codec).unwrap();
    for byte in aus[2].iter_mut().skip(8) {
        *byte = 0xFF;
    }
    for au in &aus[..5] {
        let _ = decoder.decode(au);
    }
    for (i, au) in aus.iter().enumerate().skip(5) {
        assert!(
            matches!(decoder.decode(au), Ok(Some(_))),
            "AU {i} did not decode after the IDR"
        );
    }
}

#[test]
fn frame_outlives_decoder() {
    let codec = VideoCodec::H264;
    let Some(device) = device_or_skip(codec) else {
        return;
    };
    let mut decoder = Decoder::new(&device, codec).unwrap();
    let aus = access_units(codec).unwrap();
    let frame = decoder.decode(&aus[0]).unwrap().unwrap();
    drop(decoder);
    assert_eq!(desc(&frame).Format, DXGI_FORMAT_NV12);
    drop(frame);
}

#[test]
fn frames_carry_their_au_index() {
    let codec = VideoCodec::H264;
    let Some(device) = device_or_skip(codec) else {
        return;
    };
    let mut decoder = Decoder::new(&device, codec).unwrap();
    for (i, au) in access_units(codec).unwrap().iter().enumerate() {
        let frame = decoder.decode(au).unwrap().unwrap();
        assert_eq!(frame.au_index(), i as u64);
    }
}

#[test]
fn au_is_not_lost_when_caller_holds_every_frame() {
    let codec = VideoCodec::H264;
    let Some(device) = device_or_skip(codec) else {
        return;
    };
    let mut decoder = Decoder::new(&device, codec).unwrap();
    let aus = access_units(codec).unwrap();
    let held: Vec<_> = aus[..EXTRA_SURFACES + 2]
        .iter()
        .map(|au| decoder.decode(au).unwrap().unwrap())
        .collect();
    // Every pooled frame is held: AU 6 is decoded, its picture cannot be handed out.
    assert!(decoder.decode(&aus[6]).unwrap().is_none());
    drop(held);
    for (i, au) in aus.iter().enumerate().skip(7) {
        let frame = decoder.decode(au).unwrap().unwrap();
        assert_eq!(frame.au_index(), i as u64, "AU {i} lost or late");
    }
}

#[test]
fn holds_extra_surfaces() {
    let codec = VideoCodec::H264;
    let Some(device) = device_or_skip(codec) else {
        return;
    };
    let mut decoder = Decoder::new(&device, codec).unwrap();
    let mut held = Vec::new();
    for (i, au) in access_units(codec).unwrap().iter().enumerate() {
        let frame = decoder.decode(au).unwrap();
        assert!(
            frame.is_some(),
            "AU {i}: no frame while holding {}",
            held.len()
        );
        if held.len() < 4 {
            held.extend(frame);
        }
    }
}
