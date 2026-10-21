//! D3D11 helpers: device on a given adapter, counter-cell readback, PNG snapshots, adapter list.

use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

use windows::Win32::Foundation::{HMODULE, LUID};
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_UNKNOWN;
use windows::Win32::Graphics::Direct3D11::{
    D3D11_BOX, D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_MAP_READ,
    D3D11_MAPPED_SUBRESOURCE, D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
    D3D11CreateDevice, ID3D11Device, ID3D11Device5, ID3D11DeviceContext, ID3D11DeviceContext4,
    ID3D11Texture2D,
};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT, DXGI_SAMPLE_DESC};
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE, IDXGIAdapter1, IDXGIFactory1, IDXGIFactory4,
};
use windows::core::Interface;

use crate::counter::{CELL_H, CELL_W, decode};
use crate::display::from_wide;
use crate::util::BoxResult;

pub fn device_on_adapter(
    adapter: &IDXGIAdapter1,
) -> BoxResult<(ID3D11Device5, ID3D11DeviceContext4)> {
    let (mut device, mut context) = (None, None);
    // SAFETY: out-pointers are valid locals; the adapter is alive.
    unsafe {
        D3D11CreateDevice(
            adapter,
            D3D_DRIVER_TYPE_UNKNOWN,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            None,
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            Some(&mut context),
        )?
    };
    let device: ID3D11Device = device.ok_or("no device")?;
    let context: ID3D11DeviceContext = context.ok_or("no context")?;
    Ok((device.cast()?, context.cast()?))
}

pub fn device_on_luid(luid: LUID) -> BoxResult<(ID3D11Device5, ID3D11DeviceContext4)> {
    // SAFETY: plain factory creation and lookup by LUID.
    let adapter: IDXGIAdapter1 = unsafe {
        let factory: IDXGIFactory4 = CreateDXGIFactory1()?;
        factory.EnumAdapterByLuid(luid)?
    };
    device_on_adapter(&adapter)
}

/// Hardware adapters: (description, LUID, dedicated video memory in MB).
pub fn adapters() -> BoxResult<Vec<(String, LUID, u64)>> {
    let mut list = Vec::new();
    // SAFETY: enumeration stops at DXGI_ERROR_NOT_FOUND.
    unsafe {
        let factory: IDXGIFactory1 = CreateDXGIFactory1()?;
        let mut i = 0;
        while let Ok(adapter) = factory.EnumAdapters1(i) {
            let desc = adapter.GetDesc1()?;
            if desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 == 0 {
                let mb = (desc.DedicatedVideoMemory / (1024 * 1024)) as u64;
                list.push((from_wide(&desc.Description), desc.AdapterLuid, mb));
            }
            i += 1;
        }
    }
    Ok(list)
}

pub fn luid_text(luid: LUID) -> String {
    format!("{:08X}-{:08X}", luid.HighPart, luid.LowPart)
}

fn staging(
    device: &ID3D11Device5,
    width: u32,
    height: u32,
    format: DXGI_FORMAT,
) -> BoxResult<ID3D11Texture2D> {
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
        Usage: D3D11_USAGE_STAGING,
        CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
        ..Default::default()
    };
    let mut tex = None;
    // SAFETY: valid desc and out-pointer.
    unsafe { device.CreateTexture2D(&desc, None, Some(&mut tex))? };
    tex.ok_or_else(|| "CreateTexture2D returned no texture".into())
}

/// Reads the 256x8 counter cell only (harness; the frame itself is never read by the CPU).
pub struct CellReader {
    staging: ID3D11Texture2D,
}

impl CellReader {
    pub fn new(device: &ID3D11Device5, format: DXGI_FORMAT) -> BoxResult<Self> {
        Ok(Self {
            staging: staging(device, CELL_W, CELL_H, format)?,
        })
    }

    pub fn read(&self, ctx: &ID3D11DeviceContext4, src: &ID3D11Texture2D) -> Option<u32> {
        let cell = D3D11_BOX {
            left: 0,
            top: 0,
            front: 0,
            right: CELL_W,
            bottom: CELL_H,
            back: 1,
        };
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        // SAFETY: both textures belong to `ctx`'s device; the mapped bytes are read before Unmap.
        unsafe {
            ctx.CopySubresourceRegion(&self.staging, 0, 0, 0, 0, src, 0, Some(&cell));
            ctx.Map(&self.staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
                .ok()?;
            let pitch = mapped.RowPitch as usize;
            let bytes =
                std::slice::from_raw_parts(mapped.pData.cast::<u8>(), pitch * CELL_H as usize);
            let n = decode(bytes, pitch);
            ctx.Unmap(&self.staging, 0);
            n
        }
    }
}

/// Full-frame PNG (evidence mode only, never during timing runs). BGRA8 source.
pub fn save_png(
    device: &ID3D11Device5,
    ctx: &ID3D11DeviceContext4,
    src: &ID3D11Texture2D,
    path: &Path,
) -> BoxResult<()> {
    let mut desc = D3D11_TEXTURE2D_DESC::default();
    // SAFETY: valid out-pointer.
    unsafe { src.GetDesc(&mut desc) };
    let (w, h) = (desc.Width, desc.Height);
    let tex = staging(device, w, h, desc.Format)?;
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
    // SAFETY: same device; rows are copied out before Unmap.
    unsafe {
        ctx.CopyResource(&tex, src);
        ctx.Map(&tex, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
        let pitch = mapped.RowPitch as usize;
        let bytes = std::slice::from_raw_parts(mapped.pData.cast::<u8>(), pitch * h as usize);
        for y in 0..h as usize {
            for x in 0..w as usize {
                let s = y * pitch + x * 4;
                let d = (y * w as usize + x) * 4;
                rgba[d..d + 4].copy_from_slice(&[bytes[s + 2], bytes[s + 1], bytes[s], 255]);
            }
        }
        ctx.Unmap(&tex, 0);
    }
    let mut enc = png::Encoder::new(BufWriter::new(File::create(path)?), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()?.write_image_data(&rgba)?;
    Ok(())
}
