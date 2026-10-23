# Spike S5 — Apex Legends (EAC) launch with our drivers (spec `docs/specs/0008-spike-anticheat-launch.md`)

Throwaway scripts for T18. `run-all.ps1` installs and removes drivers and certificates, so **the user runs
it** in an elevated PowerShell. Logs and screenshots go to `out/` (gitignored).

## One command
From the repo root, elevated:
`powershell -NoProfile -ExecutionPolicy Bypass -File tools\spikes\s5-anticheat\run-all.ps1`

It runs A (no drivers) → installs `vdisplay` (S1) + `vhid` (S3) → reboot (a logon task resumes it) →
B (physical main display) → C (virtual monitor as main display, switched and restored by the script) →
D (drivers removed). Per case you only: start Apex when it beeps, click through to the main menu, leave it
there, then answer y/n and type any EAC message. The script watches 5 min, takes screenshots, closes the
game. Progress lives in `out/s5-state.json`; run it again to continue, `-Restart` to start over.

Rules: never start a match or the Firing Range; nothing sends input to the game.

## Scripts
| Script | What |
|---|---|
| `run-all.ps1 [-Restart]` | The whole case order above; summary in `out/s5-summary.txt` |
| `report.ps1 -Case <name>` | Read-only: machine state, our devices and certificates, displays, EAC services, newest EAC logs (copied to `out/`) |
| `common.ps1`, `display.ps1`, `launch.ps1` | Helpers (dot-source only) |

Tests: `powershell -NoProfile -Command "Invoke-Pester tools/spikes/s5-anticheat/tests"`.
