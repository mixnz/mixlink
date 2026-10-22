# mixlink-capture

> Module card. Keep <= 50 lines. Update when the public API changes.

**Purpose:** Host frames into D3D11 textures: IddCx shared texture + fence (ADR-0006), DXGI Desktop Duplication fallback.
**Depends on:** mixlink-core.
**Hot path:** yes — one call per frame, no CPU readback.

## Planned layout
- `lib.rs` (trait `FrameSource`), `platform/windows/{idd_shared,dxgi_dup}.rs`

## Public API
- (none yet — fill in names of exported traits/types, one line each)

## Notes
- Frame path: IddCx (ADR-0006 holds, spec 0005: p99 256 µs vs DDA 422 µs). Read the IddCx surface itself
  (L3, slot `SurfaceHandle`), valid only until the driver's next acquire; frame-wait thread on MMCSS
  "Pro Audio" critical (L1). Spec 0005 → Verdict lists the T7 constraints.
- IddCx reader reference: `tools/spikes/s2-iddcx-frames/src/{shared,receiver,surface_cache}.rs`
  (DuplicateHandle from WUDFHost, device on the header's LUID, fence wait, generation change = reopen).
- DDA reader must be per-monitor DPI aware (`DuplicateOutput1` → `DXGI_ERROR_UNSUPPORTED` otherwise), run in
  the console session, and back off on `DXGI_ERROR_ACCESS_LOST`.
