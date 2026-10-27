//! A decoded video frame on the GPU, as the presenter takes it (render never depends on codec).

use mixlink_core::video::ColorMatrix;
use windows::Win32::Graphics::Direct3D11::ID3D11Texture2D;

/// NV12 picture: slice `array_index` of `texture`, visible size `width` x `height` (the texture
/// may be larger: decoders align to 16 or more). The texture must be sampleable
/// (`D3D11_BIND_SHADER_RESOURCE`) and live on the presenter's device.
#[derive(Debug, Clone, Copy)]
pub struct VideoFrame<'a> {
    pub texture: &'a ID3D11Texture2D,
    pub array_index: u32,
    pub width: u32,
    pub height: u32,
    /// YCbCr → RGB matrix the stream was encoded with (limited range).
    pub matrix: ColorMatrix,
}
