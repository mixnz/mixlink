//! Asks the driver whether it has a hardware decoder for a codec (8-bit 4:2:0 into NV12).

use windows::Win32::Graphics::Direct3D11::{
    D3D11_DECODER_PROFILE_H264_VLD_NOFGT, D3D11_DECODER_PROFILE_HEVC_VLD_MAIN, ID3D11Device,
    ID3D11VideoDevice,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_NV12;
use windows::core::{GUID, Interface};

use crate::VideoCodec;

fn profile(codec: VideoCodec) -> GUID {
    match codec {
        VideoCodec::H264 => D3D11_DECODER_PROFILE_H264_VLD_NOFGT,
        VideoCodec::Hevc => D3D11_DECODER_PROFILE_HEVC_VLD_MAIN,
    }
}

/// `false` on any failure (no video device, e.g. WARP, or no matching profile).
pub(crate) fn has_decoder(device: &ID3D11Device, codec: VideoCodec) -> bool {
    let Ok(video) = device.cast::<ID3D11VideoDevice>() else {
        return false;
    };
    let wanted = profile(codec);
    // SAFETY: `video` is a live video device; `wanted` outlives the call.
    unsafe {
        let listed = (0..video.GetVideoDecoderProfileCount())
            .any(|i| video.GetVideoDecoderProfile(i).is_ok_and(|g| g == wanted));
        listed
            && video
                .CheckVideoDecoderFormat(&wanted, DXGI_FORMAT_NV12)
                .is_ok_and(|ok| ok.as_bool())
    }
}
