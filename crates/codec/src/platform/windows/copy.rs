//! GPU-copy fallback: when the decoder pool cannot be sampled, each frame is copied on the GPU
//! into one of a few sampleable NV12 textures (never through the CPU).

use windows::Win32::Graphics::Direct3D11::{
    D3D11_BIND_SHADER_RESOURCE, D3D11_BOX, D3D11_TEXTURE2D_DESC, D3D11_USAGE_DEFAULT, ID3D11Device,
    ID3D11DeviceContext, ID3D11Texture2D,
};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_NV12, DXGI_SAMPLE_DESC};

use crate::CodecError;

/// Copy targets in rotation; the presenter holds at most one or two at a time.
const TARGETS: usize = 3;

#[derive(Debug)]
pub(crate) struct GpuCopy {
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    targets: Vec<ID3D11Texture2D>,
    size: (u32, u32),
    next: usize,
}

fn win32(context: &'static str) -> impl FnOnce(windows::core::Error) -> CodecError {
    move |source| CodecError::Win32 { context, source }
}

impl GpuCopy {
    pub(crate) fn new(device: &ID3D11Device) -> Result<Self, CodecError> {
        // SAFETY: plain getter on a live device.
        let context =
            unsafe { device.GetImmediateContext() }.map_err(win32("GetImmediateContext"))?;
        Ok(Self {
            device: device.clone(),
            context,
            targets: Vec::with_capacity(TARGETS),
            size: (0, 0),
            next: 0,
        })
    }

    /// Copies slice `index` of `src` (`width` x `height`, even) into the next target (`// HOT PATH`
    /// once the targets exist for this size).
    pub(crate) fn copy(
        &mut self,
        src: &ID3D11Texture2D,
        index: u32,
        width: u32,
        height: u32,
    ) -> Result<ID3D11Texture2D, CodecError> {
        if self.size != (width, height) || self.targets.len() < TARGETS {
            self.make_targets(width, height)?;
        }
        let target = &self.targets[self.next];
        self.next = (self.next + 1) % TARGETS;
        let region = D3D11_BOX {
            left: 0,
            top: 0,
            front: 0,
            right: width,
            bottom: height,
            back: 1,
        };
        // SAFETY: both textures are live NV12 resources on this device; `index` is a slice of
        // `src` (one mip level, so subresource == slice); the box lies inside both.
        unsafe {
            self.context
                .CopySubresourceRegion(target, 0, 0, 0, 0, src, index, Some(&region));
        }
        Ok(target.clone())
    }

    fn make_targets(&mut self, width: u32, height: u32) -> Result<(), CodecError> {
        self.targets.clear();
        let desc = D3D11_TEXTURE2D_DESC {
            Width: width,
            Height: height,
            MipLevels: 1,
            ArraySize: 1,
            Format: DXGI_FORMAT_NV12,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as u32,
            ..Default::default()
        };
        for _ in 0..TARGETS {
            let mut texture = None;
            // SAFETY: `desc` is valid for the call; `texture` is a valid out-pointer.
            unsafe { self.device.CreateTexture2D(&desc, None, Some(&mut texture)) }
                .map_err(win32("CreateTexture2D (copy target)"))?;
            let texture = texture.ok_or(CodecError::OutOfMemory)?;
            self.targets.push(texture);
        }
        self.size = (width, height);
        self.next = 0;
        Ok(())
    }
}
