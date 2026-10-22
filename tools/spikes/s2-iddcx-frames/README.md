# Spike S2: IddCx frames vs DDA (throwaway)

Spec: [0004](../../../docs/specs/0004-spike-iddcx-frames.md). Standalone Cargo package (not in the main
workspace). Output (logs, CSV, PNG) goes to `out/` (gitignored).

## Pieces
- `s2-pattern`: test pattern on the virtual monitor (user session). Frame counter in the top-left cell.
- `s2-svc`: service `MixLinkS2` (LocalSystem, Session 0). Reads the driver's shared texture ring.
- `s2-dda`: DDA probe; `s2-svc --mode dda` starts it as SYSTEM in the console session.
- `install-svc.ps1`, `uninstall-svc.ps1`, `run-case.ps1`: run elevated.

## Order
1. Build: `cargo build --release --manifest-path tools/spikes/s2-iddcx-frames/Cargo.toml`
   and `powershell -NoProfile -ExecutionPolicy Bypass -File drivers/build.ps1`.
2. Driver (elevated): `tools\spikes\s1-signing\uninstall.ps1 -Case S2-pre` if an older one is
   installed, then `tools\spikes\s1-signing\install.ps1 -Case S2`.
3. Service (elevated): `tools\spikes\s2-iddcx-frames\install-svc.ps1`.
4. In a normal terminal: `tools\spikes\s2-iddcx-frames\target\release\s2-pattern.exe --minutes 12`.
5. Cases (elevated):
   - A: `run-case.ps1 -Case A -Minutes 10` (IddCx timing)
   - B: `run-case.ps1 -Case B -Minutes 10` (DDA timing)
   - A2: `run-case.ps1 -Case A2 -Minutes 1` while switching the virtual monitor 120 Hz -> 60 Hz -> 120 Hz
   - C, D, E: `run-case.ps1 -Case C -Minutes 1` with the desktop, D with a UAC prompt open, E after Win+L
   - F: reboot, wait at the login screen for 30 s or more, then log in (service is auto-start, png mode)
6. Clean up: `uninstall-svc.ps1`, then `tools\spikes\s1-signing\uninstall.ps1 -Case S2-end`.
