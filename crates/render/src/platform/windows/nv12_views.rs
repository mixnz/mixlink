//! Shader views (luma R8, chroma R8G8) per decoder surface, made once and reused.

use windows::Win32::Graphics::Direct3D::D3D11_SRV_DIMENSION_TEXTURE2DARRAY;
use windows::Win32::Graphics::Direct3D11::{
    D3D11_SHADER_RESOURCE_VIEW_DESC, D3D11_SHADER_RESOURCE_VIEW_DESC_0, D3D11_TEX2D_ARRAY_SRV,
    D3D11_TEXTURE2D_DESC, ID3D11Device, ID3D11ShaderResourceView, ID3D11Texture2D,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_FORMAT, DXGI_FORMAT_R8_UNORM, DXGI_FORMAT_R8G8_UNORM,
};
use windows::core::Interface;

use crate::{RenderError, VideoFrame};

/// Surfaces remembered at once: a decoder pool plus copy targets fit easily.
const CACHED: usize = 16;

#[derive(Debug)]
pub(crate) struct Views {
    key: (usize, u32),
    pub(crate) luma: ID3D11ShaderResourceView,
    pub(crate) chroma: ID3D11ShaderResourceView,
    /// Visible part of the texture in UV units (decoders pad to 16 or more).
    pub(crate) uv_max: [f32; 2],
    size: (u32, u32),
}

#[derive(Debug)]
pub(crate) struct ViewCache {
    device: ID3D11Device,
    entries: [Option<Views>; CACHED],
    next: usize,
}

fn win32(context: &'static str) -> impl FnOnce(windows::core::Error) -> RenderError {
    move |source| RenderError::Win32 { context, source }
}

impl ViewCache {
    pub(crate) fn new(device: ID3D11Device) -> Self {
        Self {
            device,
            entries: std::array::from_fn(|_| None),
            next: 0,
        }
    }

    /// Views for `frame`; creates them on first sight (the views hold the texture alive, so a
    /// cached address cannot be reused by another texture).
    // HOT PATH (hit)
    pub(crate) fn get(&mut self, frame: &VideoFrame) -> Result<&Views, RenderError> {
        let key = (frame.texture.as_raw() as usize, frame.array_index);
        let size = (frame.width, frame.height);
        let hit = self
            .entries
            .iter()
            .position(|e| e.as_ref().is_some_and(|v| v.key == key && v.size == size));
        let slot = match hit {
            Some(slot) => slot,
            None => {
                let slot = self.next;
                self.next = (self.next + 1) % CACHED;
                self.entries[slot] = Some(self.create(frame, key)?);
                slot
            }
        };
        self.entries[slot]
            .as_ref()
            .ok_or_else(|| win32("view cache")(windows::core::Error::empty()))
    }

    fn create(&self, frame: &VideoFrame, key: (usize, u32)) -> Result<Views, RenderError> {
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        // SAFETY: live texture; valid out-pointer.
        unsafe { frame.texture.GetDesc(&mut desc) };
        let luma = self.view(frame.texture, frame.array_index, DXGI_FORMAT_R8_UNORM)?;
        let chroma = self.view(frame.texture, frame.array_index, DXGI_FORMAT_R8G8_UNORM)?;
        Ok(Views {
            key,
            luma,
            chroma,
            uv_max: [
                frame.width as f32 / desc.Width.max(1) as f32,
                frame.height as f32 / desc.Height.max(1) as f32,
            ],
            size: (frame.width, frame.height),
        })
    }

    fn view(
        &self,
        texture: &ID3D11Texture2D,
        slice: u32,
        format: DXGI_FORMAT,
    ) -> Result<ID3D11ShaderResourceView, RenderError> {
        let desc = D3D11_SHADER_RESOURCE_VIEW_DESC {
            Format: format,
            ViewDimension: D3D11_SRV_DIMENSION_TEXTURE2DARRAY,
            Anonymous: D3D11_SHADER_RESOURCE_VIEW_DESC_0 {
                Texture2DArray: D3D11_TEX2D_ARRAY_SRV {
                    MostDetailedMip: 0,
                    MipLevels: 1,
                    FirstArraySlice: slice,
                    ArraySize: 1,
                },
            },
        };
        let mut view = None;
        // SAFETY: live NV12 texture with the shader-resource bind flag; `desc` outlives the call.
        unsafe {
            self.device
                .CreateShaderResourceView(texture, Some(&desc), Some(&mut view))
        }
        .map_err(win32("CreateShaderResourceView"))?;
        view.ok_or_else(|| win32("CreateShaderResourceView")(windows::core::Error::empty()))
    }
}
