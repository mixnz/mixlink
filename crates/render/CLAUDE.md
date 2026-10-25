# mixlink-render

> Module card. Keep <= 50 lines. Update when the public API changes.

**Purpose:** Client presentation: D3D11 flip-model swapchain, waitable object, tearing/VRR, newest-frame-wins.
**Depends on:** mixlink-core.
**Hot path:** yes — present per frame.

## Layout
- `presenter.rs` (safe API), `video.rs` (`VideoFrame`), `viewport.rs` (aspect fit, + tests), `error.rs`.
- `platform/windows/`: `d3d11.rs` (swapchain, + `d3d11_tests.rs`), `device.rs` (device, event query),
  `nv12.rs` + `nv12.hlsl` (NV12 → BGRA pass, + `nv12_tests.rs`), `nv12_views.rs` (SRV cache), `test_util.rs`.
- `tests/no_alloc.rs`. Planned: `overlay.rs` (stats HUD).

## Public API
- `Presenter::new(Arc<W: HasWindowHandle>, w, h)`, `resize(w, h)`, `clear_and_present(rgba)`,
  `device()` (for the decoder), `present_nv12(&VideoFrame)`, `wait_gpu()` (measuring only). Windows only.
- `VideoFrame { texture, array_index, width, height }`: NV12, sampleable, on the presenter's device.
- `viewport::{fit, Rect}`.
- `RenderError`: `UnsupportedWindow`, `Handle`, `Win32 { context, source }`.

## Notes
- Takes `raw-window-handle`, never `winit`. Presenter holds the window `Arc` so the HWND outlives the swapchain.
- Device: hardware then WARP; video support asked first (dropped where it fails); debug builds try the debug
  layer first; multithread protection on (FFmpeg decodes on the same device). Size 0 = minimized, present skipped.
- NV12 pass: BT.709 limited range, bilinear, black bars, crops decoder padding (`uv_max`); shaders compiled
  at start with `D3DCompile` (vs/ps 4_0). `present_nv12` uses `Present(0, 0)`: no vblank wait.
- Tests create a hidden `STATIC` window or draw offscreen; they run on CI (WARP).
