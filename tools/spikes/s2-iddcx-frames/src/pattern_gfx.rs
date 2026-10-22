//! D3D11 objects of the `s2-pattern` window: device, flip-model swapchain, 8x8 counter blocks.

use windows::Win32::Foundation::{HMODULE, HWND};
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
use windows::Win32::Graphics::Direct3D11::{
    D3D11_BIND_SHADER_RESOURCE, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION,
    D3D11_SUBRESOURCE_DATA, D3D11_TEXTURE2D_DESC, D3D11_USAGE_IMMUTABLE, D3D11CreateDevice,
    ID3D11Device, ID3D11DeviceContext, ID3D11RenderTargetView, ID3D11Texture2D,
};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC};
use windows::Win32::Graphics::Dxgi::{
    DXGI_SCALING_STRETCH, DXGI_SWAP_CHAIN_DESC1, DXGI_SWAP_EFFECT_FLIP_DISCARD,
    DXGI_USAGE_RENDER_TARGET_OUTPUT, IDXGIDevice, IDXGIFactory2, IDXGISwapChain1,
};
use windows::core::Interface;
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::Window;

use crate::counter::BLOCK;
use crate::util::BoxResult;

pub struct Gfx {
    pub context: ID3D11DeviceContext,
    pub swapchain: IDXGISwapChain1,
    pub back: ID3D11Texture2D,
    pub rtv: ID3D11RenderTargetView,
    pub white: ID3D11Texture2D,
    pub black: ID3D11Texture2D,
}

fn block_texture(device: &ID3D11Device, value: u8) -> BoxResult<ID3D11Texture2D> {
    let pixels = vec![value; (BLOCK * BLOCK * 4) as usize];
    let desc = D3D11_TEXTURE2D_DESC {
        Width: BLOCK,
        Height: BLOCK,
        MipLevels: 1,
        ArraySize: 1,
        Format: DXGI_FORMAT_B8G8R8A8_UNORM,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Usage: D3D11_USAGE_IMMUTABLE,
        BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as u32,
        ..Default::default()
    };
    let data = D3D11_SUBRESOURCE_DATA {
        pSysMem: pixels.as_ptr().cast(),
        SysMemPitch: BLOCK * 4,
        SysMemSlicePitch: 0,
    };
    let mut tex = None;
    // SAFETY: `pixels` outlives the call and matches `desc`.
    unsafe { device.CreateTexture2D(&desc, Some(&data), Some(&mut tex))? };
    tex.ok_or_else(|| "CreateTexture2D returned no texture".into())
}

pub fn create_gfx(window: &Window) -> BoxResult<Gfx> {
    let RawWindowHandle::Win32(handle) = window.window_handle()?.as_raw() else {
        return Err("not a Win32 window".into());
    };
    let hwnd = HWND(handle.hwnd.get() as *mut core::ffi::c_void);
    let size = window.inner_size();
    let (mut device, mut context) = (None, None);
    // SAFETY: out-pointers are valid locals.
    unsafe {
        D3D11CreateDevice(
            None,
            D3D_DRIVER_TYPE_HARDWARE,
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
    let desc = DXGI_SWAP_CHAIN_DESC1 {
        Width: size.width,
        Height: size.height,
        Format: DXGI_FORMAT_B8G8R8A8_UNORM,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
        BufferCount: 2,
        Scaling: DXGI_SCALING_STRETCH,
        SwapEffect: DXGI_SWAP_EFFECT_FLIP_DISCARD,
        ..Default::default()
    };
    // SAFETY: device and hwnd are alive; the window outlives the swapchain (both owned by App).
    let (swapchain, back, rtv) = unsafe {
        let dxgi: IDXGIDevice = device.cast()?;
        let factory: IDXGIFactory2 = dxgi.GetAdapter()?.GetParent()?;
        let swapchain = factory
            .CreateSwapChainForHwnd(&device, hwnd, &desc, None, None)
            .map_err(|e| format!("CreateSwapChainForHwnd {}x{}: {e}", desc.Width, desc.Height))?;
        let back: ID3D11Texture2D = swapchain
            .GetBuffer(0)
            .map_err(|e| format!("GetBuffer: {e}"))?;
        let mut rtv = None;
        device.CreateRenderTargetView(&back, None, Some(&mut rtv))?;
        (swapchain, back, rtv.ok_or("no rtv")?)
    };
    Ok(Gfx {
        white: block_texture(&device, 255)?,
        black: block_texture(&device, 0)?,
        context,
        swapchain,
        back,
        rtv,
    })
}
