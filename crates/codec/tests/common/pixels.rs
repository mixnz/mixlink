//! Encoder test helpers: solid BGRA textures, NV12 readback, NAL unit types.
#![allow(dead_code)]

use mixlink_codec::{DecodedFrame, VideoCodec};
use windows::Win32::Graphics::Direct3D11::{
    D3D11_BIND_FLAG, D3D11_BIND_RENDER_TARGET, D3D11_BIND_SHADER_RESOURCE, D3D11_CPU_ACCESS_READ,
    D3D11_MAP_READ, D3D11_MAPPED_SUBRESOURCE, D3D11_SUBRESOURCE_DATA, D3D11_TEXTURE2D_DESC,
    D3D11_USAGE_DEFAULT, D3D11_USAGE_STAGING, ID3D11Device, ID3D11Texture2D,
};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC};

/// A `width`×`height` BGRA texture of one colour (`rgba`), made by the test, not by FFmpeg.
pub fn bgra_texture(
    device: &ID3D11Device,
    width: u32,
    height: u32,
    rgba: [u8; 4],
) -> windows::core::Result<ID3D11Texture2D> {
    let [r, g, b, a] = rgba;
    let pixels: Vec<u8> = [b, g, r, a].repeat((width * height) as usize);
    let desc = D3D11_TEXTURE2D_DESC {
        Width: width,
        Height: height,
        MipLevels: 1,
        ArraySize: 1,
        Format: DXGI_FORMAT_B8G8R8A8_UNORM,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Usage: D3D11_USAGE_DEFAULT,
        BindFlags: D3D11_BIND_FLAG(D3D11_BIND_SHADER_RESOURCE.0 | D3D11_BIND_RENDER_TARGET.0).0
            as u32,
        CPUAccessFlags: 0,
        MiscFlags: 0,
    };
    let init = D3D11_SUBRESOURCE_DATA {
        pSysMem: pixels.as_ptr().cast(),
        SysMemPitch: width * 4,
        SysMemSlicePitch: 0,
    };
    let mut texture = None;
    // SAFETY: `desc` and `init` (with `pixels`) outlive the call; valid out-pointer.
    unsafe { device.CreateTexture2D(&desc, Some(&init), Some(&mut texture)) }?;
    texture.ok_or_else(windows::core::Error::empty)
}

/// (Y, U, V) of the decoded picture at (`x`, `y`), read back through a staging copy.
pub fn read_nv12_pixel(
    device: &ID3D11Device,
    frame: &DecodedFrame,
    x: u32,
    y: u32,
) -> windows::core::Result<(u8, u8, u8)> {
    let mut desc = D3D11_TEXTURE2D_DESC::default();
    // SAFETY: live texture, valid out-pointer.
    unsafe { frame.texture().GetDesc(&mut desc) };
    desc.ArraySize = 1;
    desc.Usage = D3D11_USAGE_STAGING;
    desc.BindFlags = 0;
    desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
    desc.MiscFlags = 0;
    let mut staging = None;
    // SAFETY: valid desc and out-pointer.
    unsafe { device.CreateTexture2D(&desc, None, Some(&mut staging)) }?;
    let staging: ID3D11Texture2D = staging.ok_or_else(windows::core::Error::empty)?;
    // SAFETY: live device and resources; subresource = array slice (one mip level).
    let ctx = unsafe { device.GetImmediateContext() }?;
    let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
    // SAFETY: as above; the mapping is read inside the block and unmapped.
    unsafe {
        ctx.CopySubresourceRegion(
            &staging,
            0,
            0,
            0,
            0,
            frame.texture(),
            frame.array_index(),
            None,
        );
        ctx.Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
        let base = mapped.pData as *const u8;
        let pitch = mapped.RowPitch as usize;
        let luma = *base.add(y as usize * pitch + x as usize);
        let chroma =
            base.add(pitch * desc.Height as usize + (y / 2) as usize * pitch + (x & !1) as usize);
        let px = (luma, *chroma, *chroma.add(1));
        ctx.Unmap(&staging, 0);
        Ok(px)
    }
}

/// NAL unit types in an Annex-B access unit.
pub fn nal_types(codec: VideoCodec, au: &[u8]) -> Vec<u8> {
    let mut types = Vec::new();
    let mut i = 0;
    while i + 3 < au.len() {
        if au[i] == 0 && au[i + 1] == 0 && au[i + 2] == 1 {
            let header = au[i + 3];
            types.push(match codec {
                VideoCodec::H264 => header & 0x1F,
                VideoCodec::Hevc => (header >> 1) & 0x3F,
            });
            i += 3;
        } else {
            i += 1;
        }
    }
    types
}

pub fn has_idr(codec: VideoCodec, au: &[u8]) -> bool {
    nal_types(codec, au).iter().any(|&t| match codec {
        VideoCodec::H264 => t == 5,
        VideoCodec::Hevc => t == 19 || t == 20,
    })
}

pub fn has_sps(codec: VideoCodec, au: &[u8]) -> bool {
    nal_types(codec, au).iter().any(|&t| match codec {
        VideoCodec::H264 => t == 7,
        VideoCodec::Hevc => t == 33,
    })
}
