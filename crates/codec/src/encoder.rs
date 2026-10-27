//! Hardware video encoder (host): NVENC through FFmpeg on the capture device's textures, low latency,
//! no B-frames, IDR on request (spec 0019).

use windows::Win32::Graphics::Direct3D11::{D3D11_TEXTURE2D_DESC, ID3D11Device, ID3D11Texture2D};

use crate::platform::windows::nvenc::{AU_CAPACITY, NvencEncoder};
use crate::{CodecError, VideoCodec};

/// What to encode. The encoder must be re-created when any of it changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EncoderConfig {
    pub codec: VideoCodec,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub bitrate_kbps: u32,
}

/// One texture in, one Annex-B access unit out (SPS/PPS on every IDR).
#[derive(Debug)]
pub struct Encoder {
    inner: NvencEncoder,
    config: EncoderConfig,
    au: Vec<u8>,
}

impl Encoder {
    /// Encodes textures that live on `device`. `NoHardwareEncoder` when the GPU has no NVENC for
    /// `config.codec` (QSV / AMF come later).
    pub fn new(device: &ID3D11Device, config: &EncoderConfig) -> Result<Self, CodecError> {
        let inner = NvencEncoder::open(device, config)?;
        log::info!(
            "{:?} encoder open (NVENC, {}x{} @ {} fps, {} kbps)",
            config.codec,
            config.width,
            config.height,
            config.fps,
            config.bitrate_kbps
        );
        Ok(Self {
            inner,
            config: *config,
            au: Vec::with_capacity(AU_CAPACITY),
        })
    }

    pub fn config(&self) -> &EncoderConfig {
        &self.config
    }

    // HOT PATH
    /// Encodes a BGRA texture of the configured size. The access unit is valid until the next call;
    /// the texture is no longer read once this returns.
    pub fn encode(
        &mut self,
        texture: &ID3D11Texture2D,
        force_idr: bool,
    ) -> Result<&[u8], CodecError> {
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        // SAFETY: live texture, valid out-pointer.
        unsafe { texture.GetDesc(&mut desc) };
        if (desc.Width, desc.Height) != (self.config.width, self.config.height) {
            return Err(CodecError::FrameSize {
                expected: (self.config.width, self.config.height),
                got: (desc.Width, desc.Height),
            });
        }
        self.inner.encode(texture, force_idr, &mut self.au)?;
        Ok(&self.au)
    }
}

/// Whether `device` can encode `codec` in hardware (opens and closes a small encoder).
pub fn has_hardware_encoder(device: &ID3D11Device, codec: VideoCodec) -> bool {
    let probe = EncoderConfig {
        codec,
        width: 256,
        height: 144,
        fps: 60,
        bitrate_kbps: 1000,
    };
    NvencEncoder::open(device, &probe).is_ok()
}
