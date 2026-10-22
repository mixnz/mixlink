# Spike S3 — virtual HID input (spec `docs/specs/0006-spike-vhid.md`)

Throwaway harness for T16. Scripts that change drivers, certificates or scheduled tasks are run by the
user in an elevated PowerShell; logs go to `out/` (gitignored).

## Scripts
| Script | What |
|---|---|
| `install.ps1 -Case <name>` | Signs `target/drivers/vhid/pkg` with a fresh per-machine cert (`CN=MixLink Input Driver (this PC)`), trusts it, deletes the private key, installs `Root\MixLinkVHid` |
| `uninstall.ps1 -Case <name>` | Removes the device, driver package and certificate; ends with `RESULT CLEAN` |
| `report.ps1 -Case <name>` | Device state, the five HID collections (keyboard, mouse-rel, mouse-abs, gamepad, control), catalog signer |

## Rust harness (own `[workspace]`, not in the main build)
Build once: `cargo build --release --manifest-path tools/spikes/s3-vhid/Cargo.toml`; binaries in
`tools/spikes/s3-vhid/target/release/`. Every run writes `out/<unix-time>-<name>.txt`.

| Binary | What |
|---|---|
| `s3-latency <vhid\|sendinput\|vigem> <kbd\|rel\|abs\|pad\|wgi> [--count N]` | Feed call -> input seen by a receiver (Raw Input; XInput for `vigem pad`; `RawGameController` for `wgi`). 10 000 events, 2 ms apart; p50/p99/max, lost, driver drops. PASS = p99 <= 1 ms (+ 0 lost for vhid) |
| `s3-feed <backend> <scenario> [--delay s]` | Scripted input after a countdown: `Walk`, `Look`, `Menu`, `PadWalk`, `PadLook`, `PadMenu` |
| `s3-feed vhid --raw <hex>` | One raw control output report (driver validation) |

Latency events never type text: keys F13-F24, mouse moves return to the start, the pad moves LX only.
`vigem` needs ViGEmBus installed by the user from its author's signed release.

## Driver package
Build the package first: `powershell -NoProfile -ExecutionPolicy Bypass -File drivers/build.ps1 -Driver vhid`.
Signing helpers come from `../s1-signing/` (dot-sourced).

Tests: `powershell -NoProfile -Command "Invoke-Pester tools/spikes/s3-vhid/tests"`.
