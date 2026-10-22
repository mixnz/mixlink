# Spike S3 — virtual HID input (spec `docs/specs/0006-spike-vhid.md`)

Throwaway harness for T16. Scripts that change drivers, certificates or scheduled tasks are run by the
user in an elevated PowerShell; logs go to `out/` (gitignored).

## Scripts
| Script | What |
|---|---|
| `install.ps1 -Case <name>` | Signs `target/drivers/vhid/pkg` with a fresh per-machine cert (`CN=MixLink Input Driver (this PC)`), trusts it, deletes the private key, installs `Root\MixLinkVHid` |
| `uninstall.ps1 -Case <name>` | Removes the device, driver package and certificate; ends with `RESULT CLEAN` |
| `report.ps1 -Case <name>` | Device state, the five HID collections (keyboard, mouse-rel, mouse-abs, gamepad, control), catalog signer |

Build the package first: `powershell -NoProfile -ExecutionPolicy Bypass -File drivers/build.ps1 -Driver vhid`.
Signing helpers come from `../s1-signing/` (dot-sourced).

Tests: `powershell -NoProfile -Command "Invoke-Pester tools/spikes/s3-vhid/tests"`.
