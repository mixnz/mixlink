# mixlink-capture

> Module card. Keep <= 50 lines. Update when the public API changes.

**Purpose:** Host frames into D3D11 textures from the `vdisplay` frame share (IddCx surface + fence, ADR-0006,
spec 0019). DXGI Desktop Duplication fallback: later (own roadmap row).
**Depends on:** mixlink-core.
**Hot path:** yes — one `next` per frame, no CPU readback.

## Layout
- `header.rs` (+ tests): `FrameHeader` / `FrameSlot` = `drivers/vdisplay/SharedHeader.h` (size / offset asserts),
  names per connector. `reader.rs` (+ tests): `HeaderView`, `ReaderState::poll` → `Poll`. `surface_cache.rs`
  (+ tests). `source.rs`: `Frame`, `FrameSource`, `CaptureEvent`. `error.rs`.
- `platform/windows/`: `section.rs` (mapped header, atomics, events), `device.rs` (device on the header's LUID,
  `DriverProcess` duplicates handles out of WUDFHost), `idd_source.rs` (`IddSource`, `CapturedFrame`).
- `tests/open.rs`: missing / out-of-range connector.

## Public API
- `FrameSource` (GAT `Frame<'a>`; `next(timeout) -> Result<Option<CaptureEvent<Frame>>>`), `Frame` (`seq`,
  `width`, `height`, `present_qpc`, `acquire_qpc`, `captured`), `CaptureEvent::{Frame, Reopened { width, height }}`.
- `IddSource::open(connector)`, `device() -> Option<ID3D11Device>` (after the first `Reopened`; encode on it),
  `hold_timeouts()`, `ring_fallbacks()`. `CapturedFrame::texture()`; dropping it releases the surface.
- `CaptureError`: `NoFrameShare { connector }`, `WrongVersion { found }`, `NoSwapchain`, `Win32 { context, source }`.
- `header::*`, `reader::*`, `surface_cache::SurfaceCache` (pure, used by the Windows reader).

## Notes
- The surface stays valid only while the frame is held: the driver waits (≤ 2 frame periods) for `released_seq`
  before its next acquire. Encode, then drop the frame. Heartbeat (`reader_seq`) is bumped on every `next`;
  a reader that stops calling `next` for 500 ms is treated as gone.
- Fence: CPU wait (`SetEventOnCompletion`, ≤ 100 ms) when not yet reached, so any consumer (NVENC) sees DWM's
  pixels. Seqs grow across generations; the fence value = seq.
- No swapchain for 2 s → `NoSwapchain`. The caller sets thread priority (MMCSS "Pro Audio" critical).
- Needs SYSTEM or elevated Admin (section ACL). The S2 spike reader (`tools/spikes/s2-iddcx-frames`) is v2.
