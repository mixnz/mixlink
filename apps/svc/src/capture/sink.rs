//! The real `EncodeSink`: NVENC on the capture device, access units to the file writer.

use mixlink_capture::{CaptureError, CapturedFrame, IddSource};
use mixlink_codec::{Encoder, EncoderConfig};

use super::flow::EncodeSink;
use super::writer::WriterTx;
use crate::error::SvcError;
use crate::platform::windows::priority::raise_gpu;

#[derive(Debug)]
pub struct NvencSink {
    config: EncoderConfig,
    encoder: Option<Encoder>,
    writer: WriterTx,
    gpu_lever: bool,
}

impl NvencSink {
    /// `config.width` / `height` are replaced by each reopen's size.
    pub fn new(config: EncoderConfig, writer: WriterTx, gpu_lever: bool) -> Self {
        Self {
            config,
            encoder: None,
            writer,
            gpu_lever,
        }
    }

    pub fn dropped_aus(&self) -> u64 {
        self.writer.dropped()
    }
}

impl EncodeSink<IddSource> for NvencSink {
    fn reopen(&mut self, src: &IddSource, width: u32, height: u32) -> Result<(), SvcError> {
        self.encoder = None;
        let device = src.device().ok_or(CaptureError::NoSwapchain)?;
        if self.gpu_lever {
            raise_gpu(&device);
        }
        self.config.width = width;
        self.config.height = height;
        self.encoder = Some(Encoder::new(&device, &self.config)?);
        Ok(())
    }

    // HOT PATH
    fn encode(&mut self, frame: &CapturedFrame<'_>, force_idr: bool) -> Result<usize, SvcError> {
        let Some(encoder) = self.encoder.as_mut() else {
            return Ok(0);
        };
        let au = encoder.encode(frame.texture(), force_idr)?;
        self.writer.push(au);
        Ok(au.len())
    }
}
