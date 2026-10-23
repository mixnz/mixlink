//! D3D11 device on a vendor's adapter, frame-number grid drawn with `ClearView`, GPU idle wait.

use std::collections::HashMap;

use windows::Win32::Foundation::{HMODULE, RECT};
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_UNKNOWN;
use windows::Win32::Graphics::Direct3D11::{
    D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_CREATE_DEVICE_VIDEO_SUPPORT, D3D11_QUERY_DESC,
    D3D11_QUERY_EVENT, D3D11_RENDER_TARGET_VIEW_DESC, D3D11_RENDER_TARGET_VIEW_DESC_0,
    D3D11_RTV_DIMENSION_TEXTURE2DARRAY, D3D11_SDK_VERSION, D3D11_TEX2D_ARRAY_RTV,
    D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11DeviceContext1, ID3D11Query,
    ID3D11RenderTargetView, ID3D11Texture2D,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_FORMAT, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_R8_UNORM, DXGI_FORMAT_R8G8_UNORM,
};
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE, IDXGIAdapter1, IDXGIFactory1,
};
use windows::core::{BOOL, Interface};

use crate::args::Format;
use crate::grid::{BLOCK, draw_rects};
use crate::util::BoxResult;

pub const VENDOR_NVIDIA: u32 = 0x10DE;
pub const VENDOR_INTEL: u32 = 0x8086;

fn from_wide(w: &[u16]) -> String {
    let end = w.iter().position(|&c| c == 0).unwrap_or(w.len());
    String::from_utf16_lossy(&w[..end])
}

/// Hardware adapters: (description, vendor id, dedicated video memory in MB).
pub fn adapters() -> BoxResult<Vec<(String, u32, u64)>> {
    Ok(hardware_adapters()?
        .into_iter()
        .map(|(_, name, vendor, mb)| (name, vendor, mb))
        .collect())
}

fn hardware_adapters() -> BoxResult<Vec<(IDXGIAdapter1, String, u32, u64)>> {
    let mut list = Vec::new();
    // SAFETY: enumeration stops at DXGI_ERROR_NOT_FOUND.
    unsafe {
        let factory: IDXGIFactory1 = CreateDXGIFactory1()?;
        let mut i = 0;
        while let Ok(adapter) = factory.EnumAdapters1(i) {
            let desc = adapter.GetDesc1()?;
            if desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 == 0 {
                let mb = (desc.DedicatedVideoMemory / (1024 * 1024)) as u64;
                list.push((adapter, from_wide(&desc.Description), desc.VendorId, mb));
            }
            i += 1;
        }
    }
    Ok(list)
}

/// Index of the first adapter of `vendor` in plain DXGI enumeration (what FFmpeg's d3d11va / QSV
/// device strings mean).
pub fn adapter_index(vendor: u32) -> BoxResult<u32> {
    // SAFETY: enumeration stops at DXGI_ERROR_NOT_FOUND.
    unsafe {
        let factory: IDXGIFactory1 = CreateDXGIFactory1()?;
        let mut i = 0;
        while let Ok(adapter) = factory.EnumAdapters1(i) {
            if adapter.GetDesc1()?.VendorId == vendor {
                return Ok(i);
            }
            i += 1;
        }
    }
    Err(format!("no adapter with vendor id {vendor:#06x}").into())
}

pub fn immediate_context(device: &ID3D11Device) -> BoxResult<ID3D11DeviceContext1> {
    // SAFETY: the device is alive; GetImmediateContext returns an AddRef'd context.
    let ctx = unsafe { device.GetImmediateContext()? };
    Ok(ctx.cast()?)
}

/// Device (video support on, as FFmpeg's d3d11va and QSV need) on the first adapter of `vendor`.
pub fn device_for_vendor(vendor: u32) -> BoxResult<(ID3D11Device, ID3D11DeviceContext1, String)> {
    let (adapter, name, _, _) = hardware_adapters()?
        .into_iter()
        .find(|(_, _, v, _)| *v == vendor)
        .ok_or_else(|| format!("no adapter with vendor id {vendor:#06x}"))?;
    let (mut device, mut context) = (None, None);
    // SAFETY: out-pointers are valid locals; the adapter is alive.
    unsafe {
        D3D11CreateDevice(
            &adapter,
            D3D_DRIVER_TYPE_UNKNOWN,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT | D3D11_CREATE_DEVICE_VIDEO_SUPPORT,
            None,
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            Some(&mut context),
        )?
    };
    let device: ID3D11Device = device.ok_or("no device")?;
    let context: ID3D11DeviceContext = context.ok_or("no context")?;
    Ok((device, context.cast()?, name))
}

/// Draws frame number grids into slices of FFmpeg's pool texture; views are created once per slice.
pub struct GridPainter {
    device: ID3D11Device,
    format: Format,
    views: HashMap<(usize, u32), Vec<ID3D11RenderTargetView>>,
    query: ID3D11Query,
}

impl GridPainter {
    pub fn new(device: &ID3D11Device, format: Format) -> BoxResult<Self> {
        let desc = D3D11_QUERY_DESC {
            Query: D3D11_QUERY_EVENT,
            MiscFlags: 0,
        };
        let mut query = None;
        // SAFETY: valid desc and out-pointer.
        unsafe { device.CreateQuery(&desc, Some(&mut query))? };
        Ok(Self {
            device: device.clone(),
            format,
            views: HashMap::new(),
            query: query.ok_or("no query")?,
        })
    }

    fn view(
        &self,
        tex: &ID3D11Texture2D,
        slice: u32,
        format: DXGI_FORMAT,
    ) -> BoxResult<ID3D11RenderTargetView> {
        let desc = D3D11_RENDER_TARGET_VIEW_DESC {
            Format: format,
            ViewDimension: D3D11_RTV_DIMENSION_TEXTURE2DARRAY,
            Anonymous: D3D11_RENDER_TARGET_VIEW_DESC_0 {
                Texture2DArray: D3D11_TEX2D_ARRAY_RTV {
                    MipSlice: 0,
                    FirstArraySlice: slice,
                    ArraySize: 1,
                },
            },
        };
        let mut rtv = None;
        // SAFETY: the texture is alive and has the render-target bind flag (set on FFmpeg's pool).
        unsafe {
            self.device
                .CreateRenderTargetView(tex, Some(&desc), Some(&mut rtv))?
        };
        Ok(rtv.ok_or("no render target view")?)
    }

    /// Clears the slice to black and paints the bits of `n` white. NV12: luma 16 / 235, chroma 128.
    pub fn draw(
        &mut self,
        ctx: &ID3D11DeviceContext1,
        tex: &ID3D11Texture2D,
        slice: u32,
        n: u32,
    ) -> BoxResult<()> {
        let key = (tex.as_raw() as usize, slice);
        if !self.views.contains_key(&key) {
            let views = match self.format {
                Format::Bgra => vec![self.view(tex, slice, DXGI_FORMAT_B8G8R8A8_UNORM)?],
                Format::Nv12 => vec![
                    self.view(tex, slice, DXGI_FORMAT_R8_UNORM)?,
                    self.view(tex, slice, DXGI_FORMAT_R8G8_UNORM)?,
                ],
            };
            self.views.insert(key, views);
        }
        let views = &self.views[&key];
        let (black, white) = match self.format {
            Format::Bgra => ([0.0, 0.0, 0.0, 1.0], [1.0, 1.0, 1.0, 1.0]),
            Format::Nv12 => ([16.0 / 255.0; 4], [235.0 / 255.0; 4]),
        };
        let mut rects = [RECT::default(); 32];
        let mut count = 0;
        for (x, y, bit) in draw_rects(n) {
            if bit {
                let (x, y, b) = (x as i32, y as i32, BLOCK as i32);
                rects[count] = RECT {
                    left: x,
                    top: y,
                    right: x + b,
                    bottom: y + b,
                };
                count += 1;
            }
        }
        // SAFETY: views belong to the live pool texture; rects outlive the call.
        unsafe {
            ctx.ClearRenderTargetView(&views[0], &black);
            if count > 0 {
                ctx.ClearView(&views[0], &white, Some(&rects[..count]));
            }
            if let Some(chroma) = views.get(1) {
                ctx.ClearRenderTargetView(chroma, &[128.0 / 255.0; 4]);
            }
        }
        Ok(())
    }

    /// Flushes and spins until the GPU has finished all work queued so far.
    pub fn wait_idle(&self, ctx: &ID3D11DeviceContext1) {
        // SAFETY: the query is alive; `done` is a valid out-buffer of the size passed.
        unsafe {
            ctx.End(&self.query);
            ctx.Flush();
            let mut done = BOOL(0);
            loop {
                let ok = ctx.GetData(
                    &self.query,
                    Some((&raw mut done).cast()),
                    size_of::<BOOL>() as u32,
                    0,
                );
                if ok.is_ok() && done.as_bool() {
                    break;
                }
                std::hint::spin_loop();
            }
        }
    }
}
