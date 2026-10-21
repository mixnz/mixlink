# tools (dev only)

> Module card. Keep <= 50 lines. Update when the public API changes.

**Purpose:** Developer tools: `latency-probe` (per-stage timing report), `packet-sim` (loss/jitter/reorder proxy).
**Depends on:** any crate.
**Hot path:** no.

## Layout
- `latency-probe/`, `packet-sim/` (planned)
- `spikes/s1-signing/`: spike S1 (T14) install/uninstall/report scripts, PowerShell; see its README.
- `spikes/s2-iddcx-frames/`: spike S2 (T15) Rust package outside the workspace (pattern, receiver service,
  DDA probe) + service scripts; see its README.

## Public API
- (none yet — fill in names of exported traits/types, one line each)

## Notes
- (gotchas an AI session must know before editing this module)
