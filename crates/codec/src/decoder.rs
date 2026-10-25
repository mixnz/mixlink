//! Hardware video decoder: FFmpeg D3D11VA on the presenter's device, low delay, GPU zero-copy
//! (spec 0013).

use windows::Win32::Graphics::Direct3D11::{ID3D11Device, ID3D11Texture2D};

use crate::platform::windows::d3d11va::{D3d11vaDecoder, RawFrame};
use crate::platform::windows::probe;
use crate::{CodecError, VideoCodec};

/// Decoded frames the caller may hold at once on top of what the decoder needs.
pub const EXTRA_SURFACES: usize = 4;

/// Whether `device` has a hardware decoder for `codec` (NV12 output).
pub fn has_hardware_decoder(device: &ID3D11Device, codec: VideoCodec) -> bool {
    probe::has_decoder(device, codec)
}

/// One access unit in, at most one frame out (streams have no B-frames).
#[derive(Debug)]
pub struct Decoder {
    inner: D3d11vaDecoder,
}

impl Decoder {
    /// Decodes on `device` (the presenter's, so frames need no cross-device copy).
    /// `NoHardwareDecoder` when the GPU or driver cannot decode `codec`.
    pub fn new(device: &ID3D11Device, codec: VideoCodec) -> Result<Self, CodecError> {
        Self::open(device, codec, false)
    }

    /// Like `new`, but every frame is copied on the GPU into an own texture. The fallback `new`
    /// picks by itself when the driver cannot sample the decoder pool; forced here for diagnostics.
    pub fn with_gpu_copy(device: &ID3D11Device, codec: VideoCodec) -> Result<Self, CodecError> {
        Self::open(device, codec, true)
    }

    fn open(device: &ID3D11Device, codec: VideoCodec, copy: bool) -> Result<Self, CodecError> {
        let inner = D3d11vaDecoder::new(device, codec, copy)?;
        log::info!("{codec:?} decoder open (D3D11VA)");
        Ok(Self { inner })
    }

    /// `Ok(None)` when the decoder needs more data or the caller holds every spare frame.
    /// `Corrupt` drops this access unit; decoding continues with the next one.
    // HOT PATH
    pub fn decode(&mut self, au: &[u8]) -> Result<Option<DecodedFrame>, CodecError> {
        Ok(self.inner.decode(au)?.map(|raw| DecodedFrame { raw }))
    }

    /// Frames are copied into own textures because the pool cannot be sampled (known after the
    /// first frame).
    pub fn uses_gpu_copy(&self) -> bool {
        self.inner.uses_gpu_copy()
    }
}

/// A decoded NV12 picture on the GPU. Dropping it returns the surface to the decoder's pool.
#[derive(Debug)]
pub struct DecodedFrame {
    raw: RawFrame,
}

impl DecodedFrame {
    pub fn texture(&self) -> &ID3D11Texture2D {
        &self.raw.texture
    }

    /// Slice of `texture()` (an array texture for the decoder pool, 0 for a copy).
    pub fn array_index(&self) -> u32 {
        self.raw.index
    }

    pub fn width(&self) -> u32 {
        self.raw.width
    }

    pub fn height(&self) -> u32 {
        self.raw.height
    }
}
