# tools (dev only)

> Module card. Keep <= 50 lines. Update when the public API changes.

**Purpose:** Developer tools: `latency-probe` (per-stage timing report), `packet-sim` (loss/jitter/reorder proxy).
**Depends on:** any crate.
**Hot path:** no.

## Layout
- `latency-probe/`, `packet-sim/` (planned)
- `spikes/s1-signing/`: spike S1 (T14) install/uninstall/report scripts, PowerShell; see its README.
- `spikes/s2-iddcx-frames/`: spikes S2 (T15) and S2b (T23) Rust package outside the workspace (pattern,
  receiver service, DDA probe, `s2-ctl` check/cycle/pool) + scripts (`run-case.ps1 -Levers -Rounds`);
  see its README.
- `spikes/s3-vhid/`: spike S3 (T16) `vhid` install/uninstall/report scripts (reuse S1 signing) + Rust harness
  outside the workspace; see its README.
- `spikes/s4-encode-audio/`: spike S4 (T17) Rust package outside the workspace (FFmpeg NVENC/QSV encode and
  WASAPI loopback jobs as service `MixLinkS4` or in the user session, pooling) + scripts; see its README.

## Public API
- (none yet — fill in names of exported traits/types, one line each)

## Notes
- (gotchas an AI session must know before editing this module)
