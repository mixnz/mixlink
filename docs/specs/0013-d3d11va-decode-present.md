# 0013 — D3D11VA decode via FFmpeg, present in the stream window

**Status:** in-progress
**Date:** 2026-10-25
**Roadmap:** T8
**Plan:** [0013-d3d11va-decode-present](../plans/0013-d3d11va-decode-present.md) (local only, written by writing-plans skill)
**Crates touched:** `crates/codec`, `crates/render`, `apps/stream`, workspace `Cargo.toml`, `scripts/`, `.github/workflows/ci.yml`

## Goal
The stream window decodes H.264 / HEVC on the GPU (FFmpeg D3D11VA) and shows the newest frame,
GPU zero-copy from decoder to swapchain. Until the host encoder and network exist (T7, T9), a dev
play mode feeds a recorded bitstream file so decode and present can be seen and measured on real PCs.

## Scope
- In:
  - FFmpeg in the main workspace (`ffmpeg-sys-next`), local + CI build setup, DLLs in `dist/`.
  - `crates/codec`: `Decoder` (D3D11VA, low delay), `DecodedFrame`, `AccessUnitParser` (file mode only).
  - `crates/render`: shared device access, NV12 → BGRA pass with aspect-fit scaling, present per frame.
  - `apps/stream`: video thread (decode + present, newest frame wins), `--play` dev mode, per-stage stats.
  - `scripts/make-test-stream.sh`: makes sample `.h264` / `.hevc` files with `ffmpeg.exe` (HW encoder).
- Out:
  - Network input, launcher wiring, cursor overlay (T9); FEC / loss recovery requests (T13).
  - AV1, 10-bit / HDR, 4:4:4; mid-stream resolution change (decoder is recreated by T9 if needed).
  - Software decode fallback: no D3D11VA decoder for the codec → clear error, window closes.
  - Tearing / VRR, waitable-object frame pacing, stats overlay: phase 3 (`overlay.rs` stays planned).

## Design

### Dependencies (new, versions checked 2026-10-25)
| Crate | Version | Why |
|---|---|---|
| `ffmpeg-sys-next` | 9.0.1 | Same as spike S4. `ffmpeg-next` 9.0 has no safe API for D3D11VA hw device / frames contexts, so we use its sys layer (ADR-0001 family) behind a small `ffi` module |
| `windows` | 0.62.2 (workspace) | codec: `Win32_Graphics_Direct3D11`; render: `Win32_Graphics_Direct3D_Fxc` (`D3DCompile`) |
| `rtrb` | 0.4.0 (workspace) | SPSC ring for access units, feeder → video thread |

### Build setup
- Needs `FFMPEG_DIR` (FFmpeg 8.1 LGPL shared, BtbN) and `LIBCLANG_PATH` (bindgen). `.cargo/config.toml`
  `[env]` sets local defaults (`C:\tools\ffmpeg`, `C:\Program Files\LLVM\bin`) without `force`, so a
  shell value wins.
- `scripts/lib.sh` gains a check: missing FFmpeg / libclang → one ERROR line saying what to install;
  `gate.sh` puts `$FFMPEG_DIR/bin` on `PATH` for `cargo test` (DLLs at run time).
- CI (`lint`, `test`): download the pinned BtbN 8.1 zip (cached by URL), set `FFMPEG_DIR`; LLVM from
  the runner image (verified in the plan). `build.sh release` copies the FFmpeg DLLs next to the exes in `dist/`.

### Codec (`crates/codec`)
- `VideoCodec { H264, Hevc }` lives here for now; T9 moves it to `proto` if the handshake needs it.
- `Decoder::new(device: &ID3D11Device, codec: VideoCodec) -> Result<Decoder, CodecError>`:
  - D3D11VA hw device context wraps the **render device** (no second device, no cross-device copy).
  - Frames pool: NV12, `D3D11_BIND_DECODER | D3D11_BIND_SHADER_RESOURCE` so the presenter samples the
    decoder texture directly; `extra_hw_frames` covers frames held by the presenter.
  - Low delay: `AV_CODEC_FLAG_LOW_DELAY`, `AV_CODEC_FLAG2_FAST`, `thread_count = 1`, no reordering.
  - Fails with `CodecError::NoHardwareDecoder` when the GPU / driver has no decoder for the codec.
  - If the driver rejects shader-resource binding on the pool: fall back to a per-frame GPU
    `CopySubresourceRegion` into a sampleable NV12 texture (still GPU only), chosen once at start.
- `Decoder::decode(au: &[u8]) -> Result<Option<DecodedFrame>, CodecError>` (`// HOT PATH`): one
  reused `AVPacket` pointing at `au` (no copy), `send_packet` then `receive_frame`. One access unit in,
  at most one frame out (no B-frames). Corrupt data → `CodecError::Corrupt` (counted, frame dropped).
- `DecodedFrame` owns one `AVFrame` reference: `texture() -> &ID3D11Texture2D`, `array_index()`,
  `width()`, `height()`. Drop returns the surface to FFmpeg's pool. No `av_hwframe_transfer_data`.
- `AccessUnitParser` (`av_parser_parse2`): splits an Annex-B byte stream into access units. File mode
  only; in T9 the reassembler already delivers whole frames.
- `unsafe` only in `ffi.rs` / `platform/windows/`; `CodecError` with `thiserror`.

### Render (`crates/render`)
- Device creation adds `D3D11_CREATE_DEVICE_VIDEO_SUPPORT` and turns on multithread protection
  (FFmpeg locks around decode, S4 → QSV note). WARP fallback stays (no video support there: decode
  just reports `NoHardwareDecoder`).
- `Presenter::device() -> ID3D11Device` (a cloned COM ref) for the decoder.
- `Presenter::present_nv12(&VideoFrame) -> Result<(), RenderError>` (`// HOT PATH`), where
  `VideoFrame { texture: &ID3D11Texture2D, array_index, width, height }` is a render type (render
  never depends on codec; `apps/stream` maps one to the other).
  - Two SRVs per pool surface (R8 luma, R8G8 chroma), cached by (texture, index), made once.
  - Full-screen triangle, pixel shader: BT.709 limited range → RGB, bilinear. Viewport keeps the
    source aspect ratio; black bars. HLSL compiled once at start with `D3DCompile` (`d3dcompiler_47.dll`
    ships with Windows), so the build needs no shader tools.
  - `Present(0, 0)` on the flip-discard chain: never waits for vblank; DWM shows the newest frame.
- `clear_and_present` stays for the no-video state.

### Stream app (`apps/stream`)
- Threads:
  - main: winit events. Writes the new size into an `AtomicU64` and unparks the video thread.
  - `video` (`// HOT PATH`, MMCSS "Games"): owns `Presenter` + `Decoder`. Loop: apply a pending
    resize; pop every queued AU, decode each, keep only the newest `DecodedFrame` (older ones
    dropped, counted); present it; park with a 16 ms timeout when the queue is empty (re-present
    the last frame on resize / expose so the window never shows garbage).
  - `feeder` (play mode only): `AccessUnitParser` over the file, pushes AUs into an `rtrb` ring of
    preallocated buffers at `--fps` (high-res waitable timer). Stands in for T9's net rx lane.
- Play mode: `mixlink-stream --play <file.h264|.hevc> [--fps 120] [--loop]`; codec from the extension.
  Without `--play` the window behaves as today (solid colour, IPC as before). Errors close the window
  with `CloseReason::Error` as today.
- Stats: `core::StageStamps` per frame: `Received` (feeder push), `DecodeStart`, `DecodeEnd`,
  `Presented`. A preallocated ring of the last 10 000 frames; on close, stderr gets p50 / p99 / max per
  span plus counts (decoded, presented, dropped, corrupt). `--measure` adds a GPU event query wait
  after decode so `DecodeEnd` is GPU-complete (only for measuring; off by default).

### Test streams (`scripts/make-test-stream.sh`)
- `ffmpeg.exe` `testsrc2` (moving pattern, colour bars) 2560x1440@120, 10 s, no B-frames, GOP = 120,
  `h264_nvenc` / `hevc_nvenc` (`--encoder qsv` for Intel). Output in `out/` (gitignored), checked
  with `ffprobe -count_frames`. The LGPL build has no x264, so a HW encoder is required.

## Latency impact
- Stages: `Received` → `DecodeStart` (queue wait), `DecodeStart` → `DecodeEnd` (budget ≤ 3 ms),
  `DecodeEnd` → `Presented` (NV12 pass + `Present`, expected < 0.3 ms CPU side).
- No CPU copy of pixel data anywhere; one GPU pass (colour convert + scale).
- Measured by play mode with `--measure` on the main PC (NVIDIA) and the laptop (Intel iGPU),
  release builds, both codecs. Results go in this section.

## Acceptance criteria
- [ ] `mixlink-stream --play` shows the test pattern for H.264 and HEVC at 1440p120 with correct
      colours (bars match `ffplay` side by side) and kept aspect ratio; resize, minimize, restore and
      close keep working; `--loop` runs 10 minutes without growing memory.
- [ ] Main PC and laptop: `DecodeStart` → `DecodeEnd` (with `--measure`) p99 ≤ 3 ms for both codecs
      (laptop may drop to 1080p if its iGPU cannot hold 1440p120; recorded); all spans recorded here.
- [ ] Decode faster than the display (120 fps on 60 Hz): presented frames are the newest, older ones
      counted as dropped; the AU queue never backs up beyond one display interval.
- [ ] A corrupted AU (random bytes in the middle of the file) is counted `corrupt`, the window keeps
      running and the picture recovers at the next keyframe.
- [ ] No D3D11VA decoder (WARP device) → `NoHardwareDecoder`, window closes with `CloseReason::Error`.
- [ ] Decode → present path has no CPU copy (no `av_hwframe_transfer_data`, no staging texture) and
      our per-frame code does not allocate (counting-allocator test over the AU → present loop,
      FFmpeg's internal pools excluded).
- [ ] NV12 → BGRA pass on WARP: a known NV12 texture (limited-range BT.709 white, black, red, mid grey)
      reads back within ±2 of the expected RGB; letterbox bars are black. Runs on CI.
- [ ] Hardware decode tests skip with a log line when no hardware decoder exists (CI); they run locally.
- [ ] `dist/` from `build.sh release` runs `--play` on a PC with no FFmpeg installed.
- [ ] Module cards of `codec`, `render`, `stream` list the new API.

## Open questions
- Tearing / VRR and waitable-object pacing: phase 3 for now; T9 may pull them in if glass-to-glass needs it.
- `--play` stays in release builds (field checks on PCs without a dev setup).
