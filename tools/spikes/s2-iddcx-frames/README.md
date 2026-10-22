# Spike S2 / S2b: IddCx frames vs DDA (throwaway)

Specs: [0004](../../../docs/specs/0004-spike-iddcx-frames.md) (S2),
[0005](../../../docs/specs/0005-spike-iddcx-tail.md) (S2b: latency levers, pooled rounds).
Standalone Cargo package (not in the main workspace). Output (logs, CSV, PNG) goes to `out/` (gitignored).

## Pieces
- `s2-pattern`: test pattern on the virtual monitor (user session). Frame counter in the top-left cell.
  Keeps the display awake while it runs.
- `s2-svc`: service `MixLinkS2` (LocalSystem, Session 0). Reads the driver's shared texture ring.
- `s2-dda`: DDA probe; `s2-svc --mode dda` starts it as SYSTEM in the console session.
- `s2-ctl`: `check` (virtual monitor is main at 120 Hz), `cycle` (120 -> 60 -> 120 Hz, new swapchain),
  `pool` (pooled stats over rounds).
- `install-svc.ps1`, `uninstall-svc.ps1`, `run-case.ps1`: run elevated.

## S2b levers (spec 0005)
Registry `HKLM\SOFTWARE\MixLink\S2b`, value `Levers` (DWORD), read by the driver on every new swapchain.
`run-case.ps1 -Levers` sets it. Bits: 1 = L1 CPU priority, 2 = L2 GPU priority, 4 = L3 no copy,
8 = L4 D3D12 copy queue. Every summary shows the levers requested, active, and failed (HRESULT).

## S2b order (spec 0005 -> Measurement plan)
1. Build: `cargo build --release --manifest-path tools/spikes/s2-iddcx-frames/Cargo.toml`
   and `powershell -NoProfile -ExecutionPolicy Bypass -File drivers/build.ps1`.
2. Driver (elevated): `tools\spikes\s1-signing\uninstall.ps1 -Case S2b-pre` if an older one is
   installed, then `tools\spikes\s1-signing\install.ps1 -Case S2b`.
3. Service (elevated): `tools\spikes\s2-iddcx-frames\install-svc.ps1`.
4. Virtual monitor = main display at 120 Hz; GPU power mode at its default.
   In a normal terminal: `tools\spikes\s2-iddcx-frames\target\release\s2-pattern.exe --minutes 600`.
5. Rounds (elevated, `tools\spikes\s2-iddcx-frames\run-case.ps1 ...`; each round = 2 min A + 2 min B):
   - Smoke: `-Levers 15 -Rounds 1 -Minutes 1`, then `-Levers 0 -Rounds 1 -Minutes 1`
   - Baseline: `-Levers 0 -Rounds 3`
   - Screening: `-Levers 1`, `-Levers 2`, `-Levers 4`, `-Levers 8` (each `-Rounds 3`)
   - Final: `-Levers <kept mask> -Rounds 5`
   - Diagnostic: `-Levers <kept mask> -Rounds 3 -GpuMode max` (after setting "Prefer maximum performance")
   Pooled results: `out/<stamp>-L<mask>-<gpu-mode>-pool-A.txt` and `...-pool-B.txt`.
6. Clean up: `uninstall-svc.ps1`, `tools\spikes\s1-signing\uninstall.ps1 -Case S2b-end`,
   `reg delete HKLM\SOFTWARE\MixLink\S2b /f`.

## S2 cases (spec 0004)
- A / B: `run-case.ps1 -Case A -Minutes 10` (IddCx), `-Case B` (DDA)
- A2: `run-case.ps1 -Case A2 -Minutes 1` while switching the virtual monitor 120 Hz -> 60 Hz -> 120 Hz
- C, D, E: `run-case.ps1 -Case C -Minutes 1` with the desktop, D with a UAC prompt open, E after Win+L
- F: reboot, wait at the login screen for 30 s or more, then log in (service is auto-start, png mode)
