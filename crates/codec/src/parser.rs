//! Splits an Annex-B byte stream into access units with FFmpeg's parser (file mode only; the
//! network path gets whole frames from the reassembler). FFI module: `unsafe` lives here.

use std::ptr;

use ffmpeg_sys_next as ff;

use crate::ffi::codec_id;
use crate::{CodecError, VideoCodec};

/// Annex-B stream → access units. Output is copied into a buffer reused across calls.
pub struct AccessUnitParser {
    codec: VideoCodec,
    parser: *mut ff::AVCodecParserContext,
    ctx: *mut ff::AVCodecContext,
    out: Vec<u8>,
}

// SAFETY: the FFmpeg contexts are owned by this value and only touched through `&mut self`.
unsafe impl Send for AccessUnitParser {}

impl AccessUnitParser {
    pub fn new(codec: VideoCodec) -> Result<Self, CodecError> {
        let mut this = Self {
            codec,
            parser: ptr::null_mut(),
            ctx: ptr::null_mut(),
            out: Vec::new(),
        };
        this.open()?;
        Ok(this)
    }

    /// Consumes bytes from `input`; returns one complete access unit once the parser has one.
    /// The slice is valid until the next call.
    pub fn push(&mut self, input: &mut &[u8]) -> Result<Option<&[u8]>, CodecError> {
        let len = i32::try_from(input.len()).unwrap_or(i32::MAX);
        let (used, ready) = self.parse(input.as_ptr(), len)?;
        *input = &input[used..];
        Ok(if ready { Some(&self.out) } else { None })
    }

    /// End of stream: returns the access unit still buffered, then `None`.
    pub fn flush(&mut self) -> Result<Option<&[u8]>, CodecError> {
        let (_, ready) = self.parse(ptr::null(), 0)?;
        Ok(if ready { Some(&self.out) } else { None })
    }

    /// Drops buffered bytes so a new pass over the stream starts clean (looped playback).
    pub fn reset(&mut self) -> Result<(), CodecError> {
        self.close();
        self.open()
    }

    /// Runs the parser once; returns (bytes consumed, whether `self.out` holds a new AU).
    fn parse(&mut self, data: *const u8, len: i32) -> Result<(usize, bool), CodecError> {
        let mut out_ptr: *mut u8 = ptr::null_mut();
        let mut out_len = 0;
        // SAFETY: parser and context are live; `data` is valid for `len` bytes (or null with 0);
        // the output pointer is valid for `out_len` bytes until the next parser call.
        let used = unsafe {
            let used = ff::av_parser_parse2(
                self.parser,
                self.ctx,
                &mut out_ptr,
                &mut out_len,
                data,
                len,
                ff::AV_NOPTS_VALUE,
                ff::AV_NOPTS_VALUE,
                0,
            );
            if used >= 0 && out_len > 0 && !out_ptr.is_null() {
                self.out.clear();
                self.out
                    .extend_from_slice(std::slice::from_raw_parts(out_ptr, out_len as usize));
            }
            used
        };
        let used = crate::ffi::check(used, "av_parser_parse2")?;
        Ok((used as usize, out_len > 0))
    }

    fn open(&mut self) -> Result<(), CodecError> {
        let id = codec_id(self.codec);
        // SAFETY: plain allocations, null-checked; the decoder pointer is static FFmpeg data.
        unsafe {
            self.parser = ff::av_parser_init(id as i32);
            self.ctx = ff::avcodec_alloc_context3(ff::avcodec_find_decoder(id));
        }
        if self.parser.is_null() || self.ctx.is_null() {
            self.close();
            return Err(CodecError::OutOfMemory);
        }
        Ok(())
    }

    fn close(&mut self) {
        // SAFETY: both are owned here (or null); the free functions accept null and null the pointer.
        unsafe {
            if !self.parser.is_null() {
                ff::av_parser_close(self.parser);
                self.parser = ptr::null_mut();
            }
            ff::avcodec_free_context(&mut self.ctx);
        }
    }
}

impl Drop for AccessUnitParser {
    fn drop(&mut self) {
        self.close();
    }
}

impl std::fmt::Debug for AccessUnitParser {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AccessUnitParser")
            .field("codec", &self.codec)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
#[path = "parser_tests.rs"]
mod tests;
