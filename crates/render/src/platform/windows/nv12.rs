//! NV12 → BGRA draw pass: limited range, BT.601 or BT.709 per frame, bilinear, aspect fit
//! (specs 0013, 0019).
//! Shaders are compiled once at start (`shader::compile`).

use mixlink_core::video::ColorMatrix;
use windows::Win32::Graphics::Direct3D::D3D11_PRIMITIVE_TOPOLOGY_TRIANGLELIST;
use windows::Win32::Graphics::Direct3D11::{
    D3D11_BIND_CONSTANT_BUFFER, D3D11_BUFFER_DESC, D3D11_COMPARISON_NEVER,
    D3D11_FILTER_MIN_MAG_MIP_LINEAR, D3D11_SAMPLER_DESC, D3D11_TEXTURE_ADDRESS_CLAMP,
    D3D11_USAGE_DEFAULT, D3D11_VIEWPORT, ID3D11Buffer, ID3D11Device, ID3D11DeviceContext,
    ID3D11PixelShader, ID3D11RenderTargetView, ID3D11SamplerState, ID3D11ShaderResourceView,
    ID3D11VertexShader,
};
use windows::core::s;

use super::nv12_views::ViewCache;
use super::shader::{blob_bytes, compile, filled, win32};
use crate::viewport::fit;
use crate::{RenderError, VideoFrame};

const SHADER: &str = include_str!("nv12.hlsl");
const BLACK: [f32; 4] = [0.0, 0.0, 0.0, 1.0];

/// Shader `coef` (R from Cr, G from Cb, G from Cr, B from Cb) of each matrix.
fn coefficients(matrix: ColorMatrix) -> [f32; 4] {
    match matrix {
        ColorMatrix::Bt601 => [1.402, 0.344_136, 0.714_136, 1.772],
        ColorMatrix::Bt709 => [1.5748, 0.1873, 0.4681, 1.8556],
    }
}

#[derive(Debug)]
pub(crate) struct Nv12Pass {
    vs: ID3D11VertexShader,
    ps: ID3D11PixelShader,
    sampler: ID3D11SamplerState,
    crop: ID3D11Buffer,
    crop_value: [f32; 8],
    views: ViewCache,
}

impl Nv12Pass {
    pub(crate) fn new(device: &ID3D11Device) -> Result<Self, RenderError> {
        let vs_code = compile(SHADER, s!("nv12.hlsl"), s!("vs_main"), s!("vs_4_0"))?;
        let ps_code = compile(SHADER, s!("nv12.hlsl"), s!("ps_main"), s!("ps_4_0"))?;
        let (mut vs, mut ps, mut sampler, mut crop) = (None, None, None, None);
        let sampler_desc = D3D11_SAMPLER_DESC {
            Filter: D3D11_FILTER_MIN_MAG_MIP_LINEAR,
            AddressU: D3D11_TEXTURE_ADDRESS_CLAMP,
            AddressV: D3D11_TEXTURE_ADDRESS_CLAMP,
            AddressW: D3D11_TEXTURE_ADDRESS_CLAMP,
            ComparisonFunc: D3D11_COMPARISON_NEVER,
            MaxLOD: f32::MAX,
            ..Default::default()
        };
        let crop_desc = D3D11_BUFFER_DESC {
            ByteWidth: 32,
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: D3D11_BIND_CONSTANT_BUFFER.0 as u32,
            ..Default::default()
        };
        // SAFETY: bytecode slices come from live blobs; descriptors outlive the calls; valid
        // out-pointers.
        unsafe {
            device
                .CreateVertexShader(blob_bytes(&vs_code), None, Some(&mut vs))
                .map_err(win32("CreateVertexShader"))?;
            device
                .CreatePixelShader(blob_bytes(&ps_code), None, Some(&mut ps))
                .map_err(win32("CreatePixelShader"))?;
            device
                .CreateSamplerState(&sampler_desc, Some(&mut sampler))
                .map_err(win32("CreateSamplerState"))?;
            device
                .CreateBuffer(&crop_desc, None, Some(&mut crop))
                .map_err(win32("CreateBuffer (crop)"))?;
        }
        Ok(Self {
            vs: filled(vs, "CreateVertexShader")?,
            ps: filled(ps, "CreatePixelShader")?,
            sampler: filled(sampler, "CreateSamplerState")?,
            crop: filled(crop, "CreateBuffer (crop)")?,
            crop_value: [0.0; 8],
            views: ViewCache::new(device.clone()),
        })
    }

    /// Clears `rtv` (`dst` pixels) to black and draws `frame` fitted inside it.
    // HOT PATH
    pub(crate) fn draw(
        &mut self,
        context: &ID3D11DeviceContext,
        frame: &VideoFrame,
        rtv: &ID3D11RenderTargetView,
        dst: (u32, u32),
    ) -> Result<(), RenderError> {
        let views = self.views.get(frame)?;
        let (luma, chroma, uv_max) = (views.luma.clone(), views.chroma.clone(), views.uv_max);
        let rect = fit((frame.width, frame.height), dst);
        let [rv, gu, gv, bu] = coefficients(frame.matrix);
        let crop_value = [uv_max[0], uv_max[1], 0.0, 0.0, rv, gu, gv, bu];
        // SAFETY: every bound object is alive for the whole block; the constant buffer holds
        // 32 bytes, exactly `crop_value`.
        unsafe {
            context.OMSetRenderTargets(Some(&[Some(rtv.clone())]), None);
            context.ClearRenderTargetView(rtv, &BLACK);
            if rect.w <= 0.0 || rect.h <= 0.0 {
                return Ok(());
            }
            if crop_value != self.crop_value {
                context.UpdateSubresource(&self.crop, 0, None, crop_value.as_ptr().cast(), 0, 0);
                self.crop_value = crop_value;
            }
            context.RSSetViewports(Some(&[D3D11_VIEWPORT {
                TopLeftX: rect.x,
                TopLeftY: rect.y,
                Width: rect.w,
                Height: rect.h,
                MinDepth: 0.0,
                MaxDepth: 1.0,
            }]));
            context.IASetInputLayout(None);
            context.IASetPrimitiveTopology(D3D11_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
            context.VSSetShader(&self.vs, None);
            context.VSSetConstantBuffers(0, Some(&[Some(self.crop.clone())]));
            context.PSSetShader(&self.ps, None);
            context.PSSetConstantBuffers(0, Some(&[Some(self.crop.clone())]));
            context.PSSetShaderResources(0, Some(&[Some(luma), Some(chroma)]));
            context.PSSetSamplers(0, Some(&[Some(self.sampler.clone())]));
            context.Draw(3, 0);
            // Unbind so the decoder can write the surface again without a read/write hazard.
            let none: [Option<ID3D11ShaderResourceView>; 2] = [None, None];
            context.PSSetShaderResources(0, Some(&none));
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "nv12_tests.rs"]
mod tests;
