# 0013 — D3D11VA decode via FFmpeg, present in the stream window

**Status:** done
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
- Converted and tagged BT.709 limited range (untagged, FFmpeg uses BT.601 and `ffplay` then shows other
  colours than the stream window). Also writes `-<codec>.mkv` copies with timestamps: `ffplay` cannot
  loop the raw files.

## Latency impact
- Stages: `Received` → `DecodeStart` (queue wait), `DecodeStart` → `DecodeEnd` (budget ≤ 3 ms),
  `DecodeEnd` → `Presented` (NV12 pass + `Present`, expected < 0.3 ms CPU side).
- No CPU copy of pixel data anywhere; one GPU pass (colour convert + scale).
- Measured by play mode with `--measure` on the main PC (NVIDIA) and the laptop (Intel iGPU),
  release builds, both codecs. Results go in this section.
- Measured (2026-10-25, main PC: RTX 5070, 3840x2160 at 160 Hz, release, `--measure`, 10 s at
  2560x1440@120 from `make-test-stream.sh` NVENC, 1197 of 1200 frames presented, 3 replaced):

  | Codec | Queue p50 / p99 | Decode p50 / p99 / max | Present p50 / p99 | Total p50 / p99 |
  |---|---|---|---|---|
  | H.264 | 0.029 / 0.136 ms | 0.710 / 1.419 / 2.493 ms | 0.188 / 0.597 ms | 0.946 / 1.841 ms |
  | HEVC | 0.029 / 0.142 ms | 0.761 / 1.473 / 2.121 ms | 0.184 / 0.437 ms | 1.016 / 1.896 ms |

  Without `--measure` decode is CPU-side submit only (H.264 p50 0.36 ms, p99 0.88 ms).
- Measured (2026-10-25, laptop: Intel UHD Graphics driver 32.0.101.7088, 1920x1080 at 59 Hz, charger,
  same NVENC files, release, `--measure`). Decode only (window minimized, nothing presented):

  | File | Decode p50 / p99 / max | Queue p99 |
  |---|---|---|
  | 1080p120 H.264 | 0.661 / 2.702 / 3.102 ms | 0.191 ms |
  | 1080p120 HEVC | 0.636 / 1.965 / 7.522 ms | 0.166 ms |
  | 1440p120 H.264 | 0.591 / 1.695 / 2.843 ms | 0.164 ms |
  | 1440p120 HEVC | 0.706 / 2.273 / 7.264 ms | 0.236 ms |

  Window shown (60 Hz): about 600 of 1200 frames presented (newest wins, rest counted `dropped`), but
  `Present(0, 0)` still blocks the video thread until a back buffer frees at vblank. AUs then wait
  2–6 ms in the queue (p50, up to 8 ms) and `--measure`'s GPU wait catches the same vblank, so decode
  p99 reads 8.1–8.6 ms in some runs. That wait is presentation, not decode: frame-latency waitable
  object or tearing (Open questions) removes it.

## Acceptance criteria
- [x] `mixlink-stream --play` shows the test pattern for H.264 and HEVC at 1440p120 with correct
      colours (bars match `ffplay` side by side) and kept aspect ratio; resize, minimize, restore and
      close keep working; `--loop` runs 10 minutes without growing memory.
      Colours read from the window (`PrintWindow`) match FFmpeg's CPU decode within ±2 on all six bars
      (BT.709 test file); black bars on tall and wide windows. Resize / minimize / restore / maximize /
      close checked by the user. `--loop` 10 min (H.264): 71 976 frames, private memory 350–361 MiB, flat.
- [x] Main PC and laptop: `DecodeStart` → `DecodeEnd` (with `--measure`) p99 ≤ 3 ms for both codecs
      (laptop may drop to 1080p if its iGPU cannot hold 1440p120; recorded); all spans recorded here.
      Laptop meets it at 1080p and 1440p with nothing presented; see Latency impact for the 60 Hz wait.
- [x] Decode faster than the display (120 fps on 60 Hz): presented frames are the newest, older ones
      counted as dropped; the AU queue never backs up beyond one display interval.
      Laptop 59 Hz: ~600 of 1200 presented, ~600 `dropped`; queue max 8.1 ms < 16.7 ms.
- [ ] A corrupted AU (random bytes in the middle of the file) is counted `corrupt`, the window keeps
      running and the picture recovers at the next keyframe.
      200 KB of random bytes mid-file: the window keeps running and decodes on (1193 frames), but
      NVIDIA D3D11VA conceals the damage, so FFmpeg reports no error and `corrupt` stays 0. Recovery at
      the next IDR is covered by `codec/tests/decode.rs::corrupt_au_recovers_at_next_idr`.
- [x] No D3D11VA decoder (WARP device) → `NoHardwareDecoder` (`warp_device_has_no_decoder`); a video
      thread failure closes the window with `CloseReason::Error` (`video_failure_closes_cleanly`).
- [x] Decode → present path has no CPU copy (no `av_hwframe_transfer_data`, no staging texture) and
      our per-frame code does not allocate (counting-allocator tests in codec, render and stream,
      FFmpeg's internal pools excluded).
- [x] NV12 → BGRA pass on WARP: a known NV12 texture (limited-range BT.709 white, black, red, mid grey)
      reads back within ±2 of the expected RGB; letterbox bars are black. Runs on CI.
- [x] Hardware decode tests skip with a log line when no hardware decoder exists (CI); they run locally.
- [x] `dist/` from `build.sh release` runs `--play` on a PC with no FFmpeg installed (copied `dist/`
      run with `PATH` = Windows folders only).
- [x] Module cards of `codec`, `render`, `stream` list the new API.

## Open questions
- Tearing / VRR and waitable-object pacing: phase 3 for now; T9 may pull them in if glass-to-glass needs it.
- `--play` stays in release builds (field checks on PCs without a dev setup); without a launcher it
  ignores stdin EOF, only the window closes it.
- 60 Hz clients: `Present` blocking adds 2–6 ms (p50) before decode. Fix with the frame-latency waitable
  object (present only when a buffer is free, never block decode) in T9 or phase 3.
