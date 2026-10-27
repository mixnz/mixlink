# 0019 — Capture from the IddCx surface → NVENC encode

**Status:** in-progress
**Date:** 2026-10-27
**Roadmap:** T7
**Plan:** [0019-capture-encode](../plans/0019-capture-encode.md) (local only, written by writing-plans skill)
**Crates touched:** `drivers/vdisplay`, `crates/capture`, `crates/codec`, `apps/svc`

## Goal
The host service turns what Windows draws on a MixLink virtual monitor into an H.264 / HEVC bitstream with
NVENC, GPU only (no CPU copy, no extra GPU copy), with per-frame capture and encode timings. T9 then sends
that bitstream to the client; until then a dev command records it to a file that `mixlink-stream --play` shows.

## Scope
- In:
  - `vdisplay`: frame share per monitor (replaces the one global S2 header and `FrameShareClaim`), product
    header v3, L1 + L3 on by default, a hold hand-off so the reader may keep the surface until it is done,
    GPU sync that covers DWM's writes, Admins in the object ACL.
  - `mixlink-capture`: `FrameSource` trait + IddCx reader (header, surface cache, fence wait, reopen on
    generation change, release). Frame-wait thread on MMCSS "Pro Audio" critical (spec 0005 L1).
  - `mixlink-codec`: `Encoder` (FFmpeg `h264_nvenc` / `hevc_nvenc`, `AV_PIX_FMT_D3D11` input = the captured
    texture, low-latency settings, IDR on request), preallocated output buffer.
  - `mixlink-svc`: `capture try` dev command (admin, service stopped): adds a virtual monitor for a client id
    (spec 0015 `session::begin`), captures + encodes N seconds to an Annex-B file, prints a latency report;
    encode-thread priority levers (spec 0007 Verdict) with a hidden switch to measure each one singly.
- Out:
  - DDA fallback: row T24. It needs a console-session helper (DDA does not
    work in Session 0) and a second texture hand-off; the IddCx path covers every host with our driver.
  - QSV / AMF encode (QSV needs a decoder-bind pool + copy, spec 0007; AMF unproven): phase 2 test matrix.
    `Encoder::new` returns `NoHardwareEncoder` on a GPU without NVENC.
  - Sending, slices-as-encoded, intra-refresh, rate control from network feedback: T9 / T13 / phase 3.
  - Wiring capture into the running service per client session: T9 (`SessionStarted` / `Ended`).
  - Cursor: the IddCx surface has no hardware cursor; cursor channel is T9.
  - HDR / 10-bit, AV1, more than one capture at a time in one process (the driver side is per monitor already).

## Design

### Data flow
```
DWM → IddCx swapchain → vdisplay RunCore: acquire → GPU touch + fence signal → publish slot (surface handle)
  → svc `cap-frames` thread: wait seq event → open/cached texture → GPU wait fence → Encoder::encode
  → AU in preallocated buffer → (T7: file writer | T9: packetizer) → write ReleasedSeq + set event
  → vdisplay: next ReleaseAndAcquire
```

### Driver: frame share per monitor (header v3)
- One shared section per swapchain, named by the monitor's connector index (0–3, from `MonitorInfo`):
  `Global\MixLinkFrames-<n>`, plus events `Global\MixLinkFrameReady-<n>` and `Global\MixLinkFrameReleased-<n>`.
  `FrameShareClaim` and the `S2` names are removed; every swapchain publishes.
- SDDL `D:P(A;;GA;;;SY)(A;;GA;;;LS)(A;;GA;;;BA)`: the service (SYSTEM) and the elevated `capture try`.
- Header v3 (magic `MXF3`): v2 fields minus the lever block, plus `RefreshMhz` (hold cap), `ReaderPid`,
  `ReaderSeq` (heartbeat), `ReleasedSeq`, `HoldTimeouts`, `RingFallbacks`. Layout mirrored in `crates/capture` with size/offset asserts
  on both sides (as v2 did).
- Levers: L1 (MMCSS) and L3 (share the surface itself) always on. L2 and L4 code and the S2b registry key are
  deleted (spec 0005: L2 no gain, L4 untestable, compiled out). The ring copy stays only as the fallback
  when `CreateSharedHandle` fails on a surface (`RingFallbacks` counts it).
- GPU sync (spec 0005 L3 fence caveat): after acquire the driver copies one texel of the surface into a 1×1
  texture, then `Signal`s the shared fence with the slot's seq. The copy makes the fence wait for DWM's
  rendering; the reader `Wait`s on that value on its own context before encoding.

### Driver: hold hand-off
- The IddCx surface is valid until the next `IddCxSwapChainReleaseAndAcquireBuffer` (spec 0005). Before that
  call the driver waits on `FrameReleased-<n>` until `ReleasedSeq ≥` the seq it published, but only while a
  reader is attached (`ReaderPid` set and `ReaderSeq` changed in the last 500 ms).
- Wait cap: 2 frame periods of the current mode. On timeout `HoldTimeouts += 1` and the driver goes on
  (the reader may then see a newer frame's pixels: one possibly torn frame, counted, never a crash).
- `IddCxSwapChainFinishedProcessingFrame` is still called right after publish, so DWM can render the next
  frame into another buffer meanwhile. The hold adds latency only when encode is slower than a frame period.

### `mixlink-capture`
- `FrameSource` trait: `next_frame(timeout) -> Result<Option<CapturedFrame<'_>>>`, frame released on drop.
- `CapturedFrame`: `texture` (`ID3D11Texture2D` on the reader's device), `width`, `height`, `format`, `seq`,
  `present_qpc`, `acquire_qpc`, `captured` (`Ticks`). Dropping it writes `ReleasedSeq` and sets the event.
- `IddSource::open(connector)`: maps the header, creates the D3D11 device on the header's LUID (multithread
  protected, video support), keeps the heartbeat. Surface cache: `DuplicateHandle` from `WudfPid` +
  `OpenSharedResource1`, keyed by handle value, ≤ 8 entries, cleared on generation change.
- Generation change (mode change, swapchain recreated) → `CaptureEvent::Reopened { width, height }` so the
  caller re-creates the encoder. Magic 0 for > 2 s → `CaptureError::NoSwapchain`.
- Reference code: `tools/spikes/s2-iddcx-frames/src/{shared,receiver,surface_cache,header}.rs`.

### `mixlink-codec` encoder
- `Encoder::new(&ID3D11Device, EncoderConfig) -> Result<Encoder, CodecError>`; `EncoderConfig { codec,
  width, height, fps, bitrate_kbps }`. `encode(&ID3D11Texture2D, force_idr) -> Result<&[u8], CodecError>`
  returns the AU (Annex-B, SPS/PPS on every IDR) from a preallocated buffer, valid until the next call.
- FFmpeg: hw device context from our D3D11 device (`av_hwdevice_ctx_alloc` + `AVD3D11VADeviceContext`),
  frames context BGRA of the surface size; each `AVFrame` points at the captured texture (data[0] = texture,
  data[1] = 0). One `send_frame` → one `receive_packet`: when it returns NVENC has read the input, so the
  frame can be released.
- Settings: `preset p1`, `tune ull`, `rc cbr`, `zerolatency 1`, `delay 0`, `bf 0`, `g` infinite (IDR only on
  request or first frame), VBV = 1 frame, BT.709 limited (matches spec 0013's decoder), 4:2:0. NVENC does the
  BGRA → NV12 conversion.
- Risk: FFmpeg's NVENC may refuse a texture that is not from its own frames pool. The first plan task proves it;
  fallback = one GPU `CopyResource` into a 2-texture pool on the same device (≈ 50 µs at 1440p, measured).
- `NoHardwareEncoder(codec)` new `CodecError` value; `has_hardware_encoder(&ID3D11Device, codec)`.

### `mixlink-svc`
- `capture try --client <32 hex> --mode WxH@Hz [--topology …] [--codec h264|hevc] [--seconds 10]
  [--bitrate 30000] --out <file>`: `session::begin` → `IddSource::open` → encode loop → `session::end`
  (always, also on error / Ctrl+C). Output named `.h264` / `.hevc` so `mixlink-stream --play` reads it.
- Thread `cap-frames` does wait + encode + write (one thread: the hold hand-off makes it lockstep). File writes
  go through a preallocated SPSC queue to a writer thread (no blocking I/O in the hot loop).
- Priority levers on `cap-frames`: MMCSS "Pro Audio" critical, `IDXGIDevice::SetGPUThreadPriority(7)` on
  the reader's device, `HIGH_PRIORITY_CLASS`. Hidden `--levers mmcss,gpu,class|none` (default all).
- Report at the end: frames, IDRs, `HoldTimeouts`, `RingFallbacks`, p50 / p99 / max of
  capture (`present_qpc → Captured`), encode (`EncodeStart → EncodeEnd`), and AU size.

## Latency impact
- Capture budget ≤ 1 ms: S2b measured 256 µs p99 (present → reader) with L1 + L3; the 1-texel copy adds a
  few µs, checked in Results.
- Encode budget ≤ 3 ms: S4 measured NVENC p99 3.15 ms in Session 0 with the levers (1440p H.264). The hold
  hand-off adds nothing while encode < frame period (8.3 ms at 120 Hz).
- Measured by `capture try` (QPC stamps, `core::StageStamps` stages `Captured`, `EncodeStart`, `EncodeEnd`).

## Acceptance criteria
- [ ] `capture try` on the main PC (NVIDIA) records 10 s at 1440p120 H.264 and HEVC; both play in
  `mixlink-stream --play` with the moving content seen on the host and no torn frames.
- [ ] Recording keeps the virtual monitor's frame rate: frames ≈ refresh × seconds while content changes
  every frame (no DWM stalls from the hold).
- [ ] Report: capture p99 ≤ 1 ms, encode p99 ≤ 3.5 ms at 1440p120 (3 ms is phase 3), `HoldTimeouts` 0.
- [ ] Lever table in Results: encode p99 with all levers, none, and each one alone.
- [ ] A mode change during recording (another `--mode` from a second `display try`, or display settings) →
  encoder re-created at the new size, the file keeps playing (IDR after the change).
- [ ] With `capture try` stopped (no reader), the virtual monitor runs as before (no hold).
- [ ] Lock screen and login screen frames are in the recording on a headless host (spec 0015 open row).
- [ ] A GPU without NVENC → clear error, monitor removed, layout restored.

## Open questions
- (Answered) DDA fallback is its own row T24 (Needs T7); T9 needs only T7 for video.
- Is BGRA → NVENC conversion BT.709 limited on every driver, or do we set the colour metadata explicitly?
  (Plan checks the FFmpeg options; spec 0013's decoder assumes BT.709.)
