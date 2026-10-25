//! Name of the GPU behind a LUID, for `display status` (spec 0015).

use windows::Win32::Foundation::LUID;
use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIAdapter1, IDXGIFactory4};

/// `None` when DXGI does not know the LUID (it changes per boot) or the call fails.
pub fn gpu_name(luid: (u32, i32)) -> Option<String> {
    let luid = LUID {
        LowPart: luid.0,
        HighPart: luid.1,
    };
    // SAFETY: plain COM factory creation; no pointers passed.
    let factory: IDXGIFactory4 = unsafe { CreateDXGIFactory1() }.ok()?;
    // SAFETY: valid factory; the LUID is passed by value.
    let adapter: IDXGIAdapter1 = unsafe { factory.EnumAdapterByLuid(luid) }.ok()?;
    // SAFETY: valid adapter; GetDesc1 fills a plain struct.
    let desc = unsafe { adapter.GetDesc1() }.ok()?;
    let end = desc
        .Description
        .iter()
        .position(|&c| c == 0)
        .unwrap_or(desc.Description.len());
    Some(String::from_utf16_lossy(&desc.Description[..end]))
}
