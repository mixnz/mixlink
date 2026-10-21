# Spike S1: per-machine signed UMDF driver

Spec: [0003-spike-umdf-signing](../../../docs/specs/0003-spike-umdf-signing.md). Build first:
`powershell -NoProfile -ExecutionPolicy Bypass -File drivers/build.ps1`.

`install.ps1` and `uninstall.ps1` change the machine certificate stores and drivers: **the user runs
them** in an elevated PowerShell. Every run writes a log to `out/` (gitignored).

| Script | Elevated | What it does |
|---|---|---|
| `install.ps1 -Case <x> [-SkipTrust] [-TamperDll]` | yes | spec Design steps 1-7 |
| `uninstall.ps1 -Case <x>` | yes | removes device, driver package, certificates; prints `RESULT CLEAN` |
| `report.ps1 -Case <x> [-Since <time>]` | no | state and checks only |

## Case order
Run from the repo root, each command as
`powershell -NoProfile -ExecutionPolicy Bypass -File tools\spikes\s1-signing\<script> <args>`.
If Windows asks to trust an unverified publisher, pick "Don't install" and note it.

1. A: `install.ps1 -Case A`, then check Settings > Display shows an extra display.
2. B: reboot, then `report.ps1 -Case B`.
3. F: `uninstall.ps1 -Case F`.
4. C: `install.ps1 -Case C -SkipTrust`, then `uninstall.ps1 -Case C-cleanup`.
5. D: `install.ps1 -Case D -TamperDll`, then `uninstall.ps1 -Case D-cleanup`.
6. E: Memory integrity off (Windows Security > Device security > Core isolation), reboot,
   `install.ps1 -Case E`, `uninstall.ps1 -Case E-cleanup`, Memory integrity back on, reboot.
7. G: SAC state is in every log (`Sac`); never turn SAC off.

Tests: `powershell -NoProfile -Command "Invoke-Pester tools/spikes/s1-signing/tests"`.
