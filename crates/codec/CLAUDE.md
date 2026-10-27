# mixlink-codec

> Module card. Keep <= 50 lines. Update when the public API changes.

**Purpose:** Hardware video encode (host) and decode (client): H.264/HEVC/AV1, low-latency settings, intra-refresh.
**Depends on:** mixlink-core; `ffmpeg-sys-next` (FFmpeg 8.1 LGPL shared), `windows`.
**Hot path:** yes — encode/decode per frame; GPU textures in, bitstream slices out (and reverse).

## Layout
- `codec.rs` (`VideoCodec`), `error.rs`, `ffi.rs` (FFmpeg error text, codec ids), `parser.rs` (+ tests),
  `decoder.rs`, `encoder.rs` (safe APIs); `platform/windows/`: `probe.rs` (driver decoder profiles), `hwctx.rs`
  (D3D11VA device context, `get_format`), `d3d11va.rs` (decoder context), `frame_pool.rs`, `copy.rs`,
  `nvenc.rs` (encoder context, spec 0019).
- `tests/decode.rs`, `tests/encode.rs`, `tests/no_alloc.rs`, `tests/encode_no_alloc.rs`, `tests/common/`
  (`pixels.rs`: BGRA textures, NV12 readback, NAL types), `tests/data/tiny.{h264,hevc}` (256x144, 10 frames,
  IDR at 0 and 5). Hardware tests print `skip:` and return when the GPU has no decoder / NVENC (CI).

## Public API
- `VideoCodec` (`H264`, `Hevc`; `from_extension`).
- `CodecError`: `Ffmpeg { call, code, text }`, `NoHardwareDecoder(codec)`, `NoHardwareEncoder(codec)`,
  `FrameSize { expected, got }`, `AuTooLarge { len }`, `Corrupt`, `OutOfMemory`, `Win32`.
- `Encoder` (`new(&ID3D11Device, &EncoderConfig)`, `encode(&texture, force_idr) -> &[u8]` Annex-B AU, `config`),
  `EncoderConfig { codec, width, height, fps, bitrate_kbps }`, `has_hardware_encoder(&ID3D11Device, codec)`.
- `AccessUnitParser` (`new`, `push(&mut &[u8]) -> Option<&[u8]>`, `flush`, `reset`): Annex-B → AUs, file mode.
- `Decoder` (`new(&ID3D11Device, codec)`, `with_gpu_copy`, `decode(au) -> Option<DecodedFrame>`, `uses_gpu_copy`).
- `DecodedFrame` (`texture`, `array_index`, `width`, `height`, `au_index`, `matrix` (`core::video::ColorMatrix`
  from the stream tag; untagged = BT.709)): holds the pool surface until dropped.
- `has_hardware_decoder(&ID3D11Device, codec)`, `EXTRA_SURFACES` (4 frames the caller may hold).

## Notes
- Build: `FFMPEG_DIR` + `LIBCLANG_PATH` (defaults in `.cargo/config.toml`); DLLs on `PATH` for tests
  (`scripts/lib.sh::ffmpeg_env`). Release build scripts are optimized: bindgen overflows the stack otherwise.
- Decode runs on the presenter's device (no cross-device copy). Pool = NV12 array with
  `BIND_DECODER | BIND_SHADER_RESOURCE`; if the driver refuses, frames are GPU-copied into 3 own textures.
- `decode` copies the AU into a padded buffer (FFmpeg reads up to 64 B past the end). Low delay, 1 thread,
  no B-frames: one AU in, one frame out. An AU is never dropped: when the caller holds every pooled
  frame, the new picture is discarded instead. Hardware decoders conceal most corrupt data (no `Corrupt`).
- Spike S4 (spec 0007): in the Session 0 service, encode only matches a user-session process with raised
  priority (ADR-0005 → Encode priority). NVENC takes `AV_PIX_FMT_D3D11` frames directly (BGRA or NV12);
  NV12 arrays with the render-target flag fail (`E_INVALIDARG`). QSV: derive from a multithread-protected
  D3D11 device; pool with `D3D11_BIND_DECODER`; `hevc_qsv` has no `look_ahead` option.
- Encoder (spec 0019): the caller's BGRA texture goes straight in (`data[0]`, a dummy `buf[0]`, frames context
  of pool size 0); NVENC registers it once. One send → one packet (`delay 0`); the texture is free on return.
  NVENC converts BGRA with BT.601 whatever is asked, so streams are tagged `SMPTE170M`; the client picks the matrix.
