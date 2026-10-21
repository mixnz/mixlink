# 0006 — Always stream from a virtual display (IddCx)

**Status:** accepted · 2026-10-22

## Context
The host PC is often headless, sometimes has monitors attached. With no monitor, DWM has no
output and there is nothing to capture. Clients differ in resolution, aspect, refresh, HDR and
DPI; scaling the host's native mode blurs and letterboxes, and changing a physical monitor's
mode is fragile. An IddCx (Indirect Display) driver adds monitors with any mode we declare.

## Decision
- **Own IddCx driver** (`drivers/vdisplay`, UMDF, ADR-0004). The stream source is always a
  virtual monitor, with or without physical monitors.
- **Mode = client's display.** Handshake carries the client's displays (width, height, refresh,
  HDR, DPI); the service creates a virtual monitor with exactly that mode.
- **Stable identity per paired client:** fixed EDID / serial per client, so Windows remembers
  DPI, HDR and arrangement for it.
- **Render adapter:** the service picks the GPU (dGPU on hybrid machines) via
  `IddCxAdapterSetRenderAdapter`.
- **Monitor before login:** the service adds a virtual monitor at boot when no physical monitor
  is active, so the login screen is streamable.
- **Topology during a session** (setting, default first):
  1. virtual only: virtual monitor becomes primary, physical monitors are turned off;
  2. extend: virtual primary, physical monitors stay on.
  The previous topology is saved to disk before switching and restored on session end, and on
  service start if a crash left it switched.
- **Frame path (target):** the driver copies each swapchain frame into a shared D3D11 texture
  (one GPU copy) and signals a fence; the service opens it and feeds the encoder. No DDA/WGC.
- **Frame path (fallback):** DXGI Desktop Duplication on the virtual monitor.
- **Cursor:** hardware cursor through IddCx; shape and position go to the client on their own
  channel and the client draws the cursor.
- **Bitrate pressure** scales on the GPU before encode; the monitor mode does not change.
- Minimum host OS: Windows 11 22H2 (IddCx HDR and render adapter selection).

## Consequences
- Headless works from the first release; no dummy plug needed.
- Pixel-exact image at the client's refresh: no scaling, no letterbox, no judder from rate mismatch.
- Secure desktop (UAC, login, lock) is in the frame path for free: DWM composes it to our monitor.
- Risks to prove (spike S2): sharing a texture from WUDFHost (LocalService, Session 0) to the
  service, and frame latency vs DDA. If it fails, use the DDA fallback (needs ADR-0005 fallback).
- Replaces "Capture: WGC, DDA fallback" of ADR-0001; `mixlink-capture` becomes the IddCx frame
  reader plus DDA fallback. WGC is dropped.
- Protected content (HDCP/DRM) may show black. Accepted.
- Per-monitor DPI has no public API; setting it is deferred.
- Topology switching moves windows; restore must be tested with crash, power loss and replug.
