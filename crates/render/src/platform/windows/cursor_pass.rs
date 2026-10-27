//! Cursor draw pass (spec 0020): the shape as a quad over the video, premultiplied alpha, then the XOR
//! mask with an invert blend (`1 - dst` where the mask is white).

use windows::Win32::Graphics::Direct3D::D3D11_PRIMITIVE_TOPOLOGY_TRIANGLESTRIP;
use windows::Win32::Graphics::Direct3D11::{
    D3D11_BIND_CONSTANT_BUFFER, D3D11_BIND_SHADER_RESOURCE, D3D11_BLEND, D3D11_BLEND_DESC,
    D3D11_BLEND_INV_DEST_COLOR, D3D11_BLEND_INV_SRC_ALPHA, D3D11_BLEND_ONE, D3D11_BLEND_OP_ADD,
    D3D11_BUFFER_DESC, D3D11_COLOR_WRITE_ENABLE_ALL, D3D11_COMPARISON_NEVER,
    D3D11_FILTER_MIN_MAG_MIP_POINT, D3D11_RENDER_TARGET_BLEND_DESC, D3D11_SAMPLER_DESC,
    D3D11_SUBRESOURCE_DATA, D3D11_TEXTURE_ADDRESS_CLAMP, D3D11_TEXTURE2D_DESC, D3D11_USAGE_DEFAULT,
    D3D11_USAGE_IMMUTABLE, D3D11_VIEWPORT, ID3D11BlendState, ID3D11Buffer, ID3D11Device,
    ID3D11DeviceContext, ID3D11PixelShader, ID3D11RenderTargetView, ID3D11SamplerState,
    ID3D11ShaderResourceView, ID3D11Texture2D, ID3D11VertexShader,
};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC};
use windows::core::s;

use super::shader::{blob_bytes, compile, filled, win32};
use crate::RenderError;
use crate::cursor::{CursorDraw, CursorImage, cursor_rect};

const SHADER: &str = include_str!("cursor.hlsl");

/// The current shape on the GPU.
#[derive(Debug)]
struct Loaded {
    size: (u32, u32),
    hot: (u32, u32),
    normal: ID3D11ShaderResourceView,
    invert: Option<ID3D11ShaderResourceView>,
}

#[derive(Debug)]
pub(crate) struct CursorPass {
    vs: ID3D11VertexShader,
    ps: ID3D11PixelShader,
    sampler: ID3D11SamplerState,
    quad: ID3D11Buffer,
    premultiplied: ID3D11BlendState,
    invert: ID3D11BlendState,
    shape: Option<Loaded>,
}

fn blend(device: &ID3D11Device, src: D3D11_BLEND) -> Result<ID3D11BlendState, RenderError> {
    let mut desc = D3D11_BLEND_DESC::default();
    desc.RenderTarget[0] = D3D11_RENDER_TARGET_BLEND_DESC {
        BlendEnable: true.into(),
        SrcBlend: src,
        DestBlend: D3D11_BLEND_INV_SRC_ALPHA,
        BlendOp: D3D11_BLEND_OP_ADD,
        SrcBlendAlpha: D3D11_BLEND_ONE,
        DestBlendAlpha: D3D11_BLEND_INV_SRC_ALPHA,
        BlendOpAlpha: D3D11_BLEND_OP_ADD,
        RenderTargetWriteMask: D3D11_COLOR_WRITE_ENABLE_ALL.0 as u8,
    };
    let mut state = None;
    // SAFETY: `desc` outlives the call; valid out-pointer.
    unsafe { device.CreateBlendState(&desc, Some(&mut state)) }
        .map_err(win32("CreateBlendState"))?;
    filled(state, "CreateBlendState")
}

fn texture_view(
    device: &ID3D11Device,
    size: (u32, u32),
    bgra: &[u8],
) -> Result<ID3D11ShaderResourceView, RenderError> {
    let desc = D3D11_TEXTURE2D_DESC {
        Width: size.0,
        Height: size.1,
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
        pSysMem: bgra.as_ptr().cast(),
        SysMemPitch: size.0 * 4,
        SysMemSlicePitch: 0,
    };
    let (mut texture, mut view): (Option<ID3D11Texture2D>, _) = (None, None);
    // SAFETY: `bgra` holds `size.0 * 4 * size.1` bytes (`convert_shape` packs it); descriptors and
    // data outlive the calls; valid out-pointers.
    unsafe {
        device
            .CreateTexture2D(&desc, Some(&data), Some(&mut texture))
            .map_err(win32("CreateTexture2D (cursor)"))?;
        let texture = filled(texture, "CreateTexture2D (cursor)")?;
        device
            .CreateShaderResourceView(&texture, None, Some(&mut view))
            .map_err(win32("CreateShaderResourceView (cursor)"))?;
    }
    filled(view, "CreateShaderResourceView (cursor)")
}

impl CursorPass {
    pub(crate) fn new(device: &ID3D11Device) -> Result<Self, RenderError> {
        let vs_code = compile(SHADER, s!("cursor.hlsl"), s!("vs_main"), s!("vs_4_0"))?;
        let ps_code = compile(SHADER, s!("cursor.hlsl"), s!("ps_main"), s!("ps_4_0"))?;
        let (mut vs, mut ps, mut sampler, mut quad) = (None, None, None, None);
        let sampler_desc = D3D11_SAMPLER_DESC {
            Filter: D3D11_FILTER_MIN_MAG_MIP_POINT,
            AddressU: D3D11_TEXTURE_ADDRESS_CLAMP,
            AddressV: D3D11_TEXTURE_ADDRESS_CLAMP,
            AddressW: D3D11_TEXTURE_ADDRESS_CLAMP,
            ComparisonFunc: D3D11_COMPARISON_NEVER,
            MaxLOD: f32::MAX,
            ..Default::default()
        };
        let quad_desc = D3D11_BUFFER_DESC {
            ByteWidth: 16,
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: D3D11_BIND_CONSTANT_BUFFER.0 as u32,
            ..Default::default()
        };
        // SAFETY: bytecode slices come from live blobs; descriptors outlive the calls; valid
        // out-pointers.
        unsafe {
            device
                .CreateVertexShader(blob_bytes(&vs_code), None, Some(&mut vs))
                .map_err(win32("CreateVertexShader (cursor)"))?;
            device
                .CreatePixelShader(blob_bytes(&ps_code), None, Some(&mut ps))
                .map_err(win32("CreatePixelShader (cursor)"))?;
            device
                .CreateSamplerState(&sampler_desc, Some(&mut sampler))
                .map_err(win32("CreateSamplerState (cursor)"))?;
            device
                .CreateBuffer(&quad_desc, None, Some(&mut quad))
                .map_err(win32("CreateBuffer (cursor)"))?;
        }
        Ok(Self {
            vs: filled(vs, "CreateVertexShader (cursor)")?,
            ps: filled(ps, "CreatePixelShader (cursor)")?,
            sampler: filled(sampler, "CreateSamplerState (cursor)")?,
            quad: filled(quad, "CreateBuffer (cursor)")?,
            premultiplied: blend(device, D3D11_BLEND_ONE)?,
            invert: blend(device, D3D11_BLEND_INV_DEST_COLOR)?,
            shape: None,
        })
    }

    /// Uploads a new shape (`None`: no cursor). Not per frame: textures are created here.
    pub(crate) fn set_image(
        &mut self,
        device: &ID3D11Device,
        image: Option<&CursorImage>,
    ) -> Result<(), RenderError> {
        self.shape = None;
        let Some(image) = image else {
            return Ok(());
        };
        let size = (image.width, image.height);
        let invert = match &image.invert {
            Some(mask) => Some(texture_view(device, size, mask)?),
            None => None,
        };
        self.shape = Some(Loaded {
            size,
            hot: image.hot,
            normal: texture_view(device, size, &image.normal)?,
            invert,
        });
        Ok(())
    }

    /// Draws the cursor over what `rtv` (`dst` pixels) holds; `video` = the frame size it is fitted from.
    // HOT PATH
    pub(crate) fn draw(
        &self,
        context: &ID3D11DeviceContext,
        rtv: &ID3D11RenderTargetView,
        dst: (u32, u32),
        video: (u32, u32),
        at: CursorDraw,
    ) {
        let Some(shape) = &self.shape else {
            return;
        };
        let rect = cursor_rect(video, dst, shape.size, shape.hot, (at.x, at.y));
        if !at.visible || rect.w <= 0.0 || dst.0 == 0 || dst.1 == 0 {
            return;
        }
        let (w, h) = (dst.0 as f32, dst.1 as f32);
        let ndc = [
            rect.x / w * 2.0 - 1.0,
            1.0 - rect.y / h * 2.0,
            (rect.x + rect.w) / w * 2.0 - 1.0,
            1.0 - (rect.y + rect.h) / h * 2.0,
        ];
        // SAFETY: every bound object is alive for the whole block; the constant buffer holds 16 bytes,
        // exactly `ndc`.
        unsafe {
            context.UpdateSubresource(&self.quad, 0, None, ndc.as_ptr().cast(), 0, 0);
            context.OMSetRenderTargets(Some(&[Some(rtv.clone())]), None);
            context.RSSetViewports(Some(&[D3D11_VIEWPORT {
                TopLeftX: 0.0,
                TopLeftY: 0.0,
                Width: w,
                Height: h,
                MinDepth: 0.0,
                MaxDepth: 1.0,
            }]));
            context.IASetInputLayout(None);
            context.IASetPrimitiveTopology(D3D11_PRIMITIVE_TOPOLOGY_TRIANGLESTRIP);
            context.VSSetShader(&self.vs, None);
            context.VSSetConstantBuffers(0, Some(&[Some(self.quad.clone())]));
            context.PSSetShader(&self.ps, None);
            context.PSSetSamplers(0, Some(&[Some(self.sampler.clone())]));
            context.OMSetBlendState(&self.premultiplied, None, u32::MAX);
            context.PSSetShaderResources(0, Some(&[Some(shape.normal.clone())]));
            context.Draw(4, 0);
            if let Some(invert) = &shape.invert {
                context.OMSetBlendState(&self.invert, None, u32::MAX);
                context.PSSetShaderResources(0, Some(&[Some(invert.clone())]));
                context.Draw(4, 0);
            }
            context.OMSetBlendState(None, None, u32::MAX);
            context.PSSetShaderResources(0, Some(&[None]));
        }
    }
}

#[cfg(test)]
#[path = "cursor_pass_tests.rs"]
mod tests;
