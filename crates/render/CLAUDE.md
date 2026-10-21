# mixlink-render

> Module card. Keep <= 50 lines. Update when the public API changes.

**Purpose:** Client presentation: D3D11 flip-model swapchain, waitable object, tearing/VRR, newest-frame-wins.
**Depends on:** mixlink-core.
**Hot path:** yes — present per frame.

## Planned layout
- `presenter.rs` (safe API), `error.rs`, `platform/windows/d3d11.rs` (+ `d3d11_tests.rs`); later `overlay.rs` (stats HUD)

## Public API
- `Presenter::new(Arc<W: HasWindowHandle>, w, h)`, `resize(w, h)`, `clear_and_present(rgba)` (Windows only).
- `RenderError`: `UnsupportedWindow`, `Handle`, `Win32 { context, source }`.

## Notes
- Takes `raw-window-handle`, never `winit`. Presenter holds the window `Arc` so the HWND outlives the swapchain.
- Device: hardware then WARP; debug builds try the D3D11 debug layer first. Size 0 = minimized, present skipped.
- Tests create a hidden `STATIC` window; they run on CI (WARP).
