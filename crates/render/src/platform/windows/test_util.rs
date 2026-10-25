//! Test helpers: WARP device, NV12 textures from bands of colour, offscreen BGRA read-back.

use windows::Win32::Foundation::HMODULE;
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_WARP;
use windows::Win32::Graphics::Direct3D11::{
    D3D11_BIND_RENDER_TARGET, D3D11_BIND_SHADER_RESOURCE, D3D11_CPU_ACCESS_READ,
    D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_MAP_READ, D3D11_MAPPED_SUBRESOURCE, D3D11_SDK_VERSION,
    D3D11_SUBRESOURCE_DATA, D3D11_TEXTURE2D_DESC, D3D11_USAGE_DEFAULT, D3D11_USAGE_STAGING,
    D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_FORMAT, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_NV12, DXGI_SAMPLE_DESC,
};

pub(crate) fn warp_device() -> (ID3D11Device, ID3D11DeviceContext) {
    let mut device = None;
    let mut context = None;
    // SAFETY: valid out-pointers; no adapter or module passed.
    unsafe {
        D3D11CreateDevice(
            None,
            D3D_DRIVER_TYPE_WARP,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            None,
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            Some(&mut context),
        )
    }
    .unwrap();
    (device.unwrap(), context.unwrap())
}

fn texture(
    device: &ID3D11Device,
    (width, height): (u32, u32),
    format: DXGI_FORMAT,
    bind: u32,
    init: Option<(&[u8], u32)>,
    staging: bool,
) -> ID3D11Texture2D {
    let desc = D3D11_TEXTURE2D_DESC {
        Width: width,
        Height: height,
        MipLevels: 1,
        ArraySize: 1,
        Format: format,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Usage: if staging {
            D3D11_USAGE_STAGING
        } else {
            D3D11_USAGE_DEFAULT
        },
        BindFlags: bind,
        CPUAccessFlags: if staging {
            D3D11_CPU_ACCESS_READ.0 as u32
        } else {
            0
        },
        ..Default::default()
    };
    let data = init.map(|(bytes, pitch)| D3D11_SUBRESOURCE_DATA {
        pSysMem: bytes.as_ptr().cast(),
        SysMemPitch: pitch,
        SysMemSlicePitch: 0,
    });
    let mut out = None;
    // SAFETY: `desc` and `data` (pointing into `init`) outlive the call; valid out-pointer.
    unsafe { device.CreateTexture2D(&desc, data.as_ref().map(|d| d as *const _), Some(&mut out)) }
        .unwrap();
    out.unwrap()
}

/// NV12 texture `width` x `height` made of horizontal bands of equal height, each (Y, Cb, Cr).
pub(crate) fn nv12_bands(
    device: &ID3D11Device,
    width: u32,
    height: u32,
    bands: &[(u8, u8, u8)],
) -> ID3D11Texture2D {
    let (w, h) = (width as usize, height as usize);
    let band = h / bands.len();
    let mut bytes = vec![0u8; w * h * 3 / 2];
    for row in 0..h {
        bytes[row * w..(row + 1) * w].fill(bands[row / band].0);
    }
    for row in 0..h / 2 {
        let (_, cb, cr) = bands[row * 2 / band];
        let start = w * h + row * w;
        for px in bytes[start..start + w].chunks_mut(2) {
            px.copy_from_slice(&[cb, cr]);
        }
    }
    let bind = D3D11_BIND_SHADER_RESOURCE.0 as u32;
    texture(
        device,
        (width, height),
        DXGI_FORMAT_NV12,
        bind,
        Some((&bytes, width)),
        false,
    )
}

pub(crate) fn bgra_target(device: &ID3D11Device, width: u32, height: u32) -> ID3D11Texture2D {
    let bind = D3D11_BIND_RENDER_TARGET.0 as u32;
    texture(
        device,
        (width, height),
        DXGI_FORMAT_B8G8R8A8_UNORM,
        bind,
        None,
        false,
    )
}

/// Copies `target` to a staging texture and returns pixel (x, y) as (R, G, B).
pub(crate) fn read_pixel(
    device: &ID3D11Device,
    context: &ID3D11DeviceContext,
    target: &ID3D11Texture2D,
    (width, height): (u32, u32),
    (x, y): (u32, u32),
) -> (u8, u8, u8) {
    let staging = texture(
        device,
        (width, height),
        DXGI_FORMAT_B8G8R8A8_UNORM,
        0,
        None,
        true,
    );
    let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
    // SAFETY: same-size, same-format textures; the mapping is read within its row pitch and
    // unmapped before `staging` drops.
    unsafe {
        context.CopyResource(&staging, target);
        context
            .Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
            .unwrap();
        let row = mapped
            .pData
            .cast::<u8>()
            .add((y * mapped.RowPitch) as usize);
        let px = row.add(x as usize * 4);
        let bgr = (*px.add(2), *px.add(1), *px);
        context.Unmap(&staging, 0);
        bgr
    }
}
