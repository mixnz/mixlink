# mixlink-render

> Module card. Keep <= 50 lines. Update when the public API changes.

**Purpose:** Client presentation: D3D11 flip-model swapchain, waitable object, tearing/VRR, newest-frame-wins.
**Depends on:** mixlink-core.
**Hot path:** yes — present per frame.

## Layout
- `presenter.rs` (safe API), `video.rs` (`VideoFrame`), `viewport.rs` (aspect fit, + tests), `cursor.rs` (pure, + tests), `error.rs`.
- `platform/windows/`: `d3d11.rs` (swapchain, + `d3d11_tests.rs`), `device.rs` (device, event query),
  `nv12.rs` + `nv12.hlsl` (NV12 → BGRA pass, + `nv12_tests.rs`), `nv12_views.rs` (SRV cache), `test_util.rs`,
  `cursor_pass.rs` + `cursor.hlsl` (spec 0020, + tests), `shader.rs` (`compile`, `blob_bytes`, `filled`, `win32`).
- `tests/no_alloc.rs`. Planned: `overlay.rs` (stats HUD).

## Public API
- `Presenter::new(Arc<W: HasWindowHandle>, w, h)`, `resize(w, h)`, `clear_and_present(rgba)`,
  `device()` (for the decoder), `present_nv12(&VideoFrame)`, `wait_frame_ready(timeout)` (frame-latency waitable, max
  latency 1, spec 0020: present only after `true`), `wait_gpu()` (measuring only). Windows only.
  `set_cursor(Option<&CursorImage>)` (shape change only), `move_cursor(Option<CursorDraw>)`: drawn on each `present_nv12`.
- `cursor` (spec 0020): `convert_shape(kind, w, h, hot, pitch, px) -> CursorImage` (premultiplied + invert mask for
  XOR), `CursorKind`, `CursorDraw { x, y, visible }` (hotspot, video px), `cursor_rect`, `CURSOR_MAX_SIDE`.
- `VideoFrame { texture, array_index, width, height, matrix }`: NV12, sampleable, on the presenter's device;
  `matrix` = `core::video::ColorMatrix` (BT.601 or BT.709, limited range) from the decoder.
- `viewport::{fit, Rect}`.
- `RenderError`: `UnsupportedWindow`, `Handle`, `CursorShape`, `Win32 { context, source }`.

## Notes
- Takes `raw-window-handle`, never `winit`. Presenter holds the window `Arc` so the HWND outlives the swapchain.
- Device: hardware then WARP; video support asked first (dropped where it fails); debug builds try the debug
  layer first; multithread protection on (FFmpeg decodes on the same device). Size 0 = minimized, present skipped.
- NV12 pass: BT.601 / BT.709 per frame (constant buffer), limited range, bilinear, black bars, crops decoder padding (`uv_max`); shaders compiled
  at start with `D3DCompile` (vs/ps 4_0). `present_nv12` uses `Present(0, 0)`; the waitable keeps it from blocking.
- Tests create a hidden `STATIC` window or draw offscreen; they run on CI (WARP).
