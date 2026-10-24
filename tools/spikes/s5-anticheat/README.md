# Spike S5 — anti-cheat titles launch with our drivers (spec `docs/specs/0008-spike-anticheat-launch.md`)

Throwaway scripts for T18. Games: Apex Legends (EA Javelin Anticheat) and Elden Ring (Easy Anti-Cheat).
`run-all.ps1` installs and removes drivers and certificates, so **the user runs it** in an elevated
PowerShell. Logs and screenshots go to `out/` (gitignored).

## One command
From the repo root, elevated:
`powershell -NoProfile -ExecutionPolicy Bypass -File tools\spikes\s5-anticheat\run-all.ps1`

It runs A (no drivers) → installs `vdisplay` (S1) + `vhid` (S3) → reboot (a logon task resumes it) →
B (physical main display) → C (virtual monitor as main display, switched and restored by the script) →
D (drivers removed). Each case runs Apex, then Elden Ring. Per game you only: start it from Steam when the
script beeps (Elden Ring normally, not in offline mode), click through to the main menu, leave it there;
after 5 min the script closes the game on purpose (not a crash), comes back to the front, and asks y/n plus
any anti-cheat message. Progress lives in `out/s5-state.json`; run it again to continue, `-Restart` to
start over.

Rules: never start a match, the Firing Range, or load a save; nothing sends input to the games.

## Scripts
| Script | What |
|---|---|
| `run-all.ps1 [-Restart]` | The whole case order above; summary in `out/s5-summary.txt` |
| `report.ps1 -Case <name>` | Read-only: machine state, our devices and certificates, displays, each game's anti-cheat service and newest logs (copied to `out/`) |
| `common.ps1`, `display.ps1`, `launch.ps1` | Helpers (dot-source only); games and anti-cheat services in `$S5Games` |

Tests: `powershell -NoProfile -Command "Invoke-Pester tools/spikes/s5-anticheat/tests"`.
