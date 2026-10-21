# mixlink-capture

> Module card. Keep <= 50 lines. Update when the public API changes.

**Purpose:** Host screen capture into D3D11 textures (Windows.Graphics.Capture, DXGI Desktop Duplication fallback).
**Depends on:** mixlink-core.
**Hot path:** yes — one call per frame, no CPU readback.

## Planned layout
- `lib.rs` (trait `FrameSource`), `platform/windows/{wgc,dxgi_dup}.rs`

## Public API
- (none yet — fill in names of exported traits/types, one line each)

## Notes
- (gotchas an AI session must know before editing this module)
