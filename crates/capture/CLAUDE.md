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
- Frame path not final: spike S2 (spec 0004) passed everything but p99 latency; T23 decides IddCx vs DDA.
- IddCx reader reference: `tools/spikes/s2-iddcx-frames/src/{shared,receiver}.rs` (DuplicateHandle from
  WUDFHost, device on the header's LUID, fence wait, generation change = reopen).
- DDA reader must be per-monitor DPI aware (`DuplicateOutput1` → `DXGI_ERROR_UNSUPPORTED` otherwise), run in
  the console session, and back off on `DXGI_ERROR_ACCESS_LOST`.
