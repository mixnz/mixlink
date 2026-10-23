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
| `s3-latency <vhid\|sendinput\|vigem> <kbd\|rel\|abs\|pad\|wgi> [--count N]` | Feed call -> input seen by a receiver (Raw Input; XInput for `vigem pad`; `RawGameController` for `wgi`). 10 000 events, 2 ms apart; p50/p99/max, lost, driver drops. PASS = something received, p99 <= 1 ms, and 0 lost on Raw Input paths (XInput / WGI are polled state and may skip one). Arrivals are matched to send times after the run, so stale readings are ignored |
| `s3-feed <backend> <scenario> [--delay s]` | Scripted input after a countdown: `Walk`, `Look`, `Menu`, `PadWalk`, `PadLook`, `PadMenu` |
| `s3-feed vhid --raw <hex>` | One raw control output report (driver validation) |
| `s3-feed <vhid\|vigem> --pad-interactive` | Type `<control> [hold_ms]` + Enter (`a`, `lb`, `guide`, `share`, `b13`-`b16`, `up`, `lt`, `ls-up`, ...): pressed for 0.5 s or `hold_ms`. `prime` nudges each trigger / stick off rest and back first. For mapping wizards such as Steam "Begin Setup" |
| `s3-apis` | Which APIs see which device: Raw Input, DirectInput 8, WGI (`RawGameController`, `Gamepad`), XInput 0-3 |
| `s3-hook [--seconds N]` | Low-level hooks: counts keyboard / mouse events with and without the injected flags |

`run-system.ps1 -Scenario <UacTab|LockSpace> [-Delay s]` (elevated): runs the release `s3-feed vhid` as
LocalSystem in Session 0 via a one-shot scheduled task, then removes the task. `UacTab` = mouse move +
Tab x3, `LockSpace` = mouse move + Space; never Enter, never text.

Latency events never type text: keys F13-F24, mouse moves return to the start, the pad moves LX only.
`vigem` needs ViGEmBus installed by the user from its author's signed release.

## Driver package
Build the package first: `powershell -NoProfile -ExecutionPolicy Bypass -File drivers/build.ps1 -Driver vhid`.
Signing helpers come from `../s1-signing/` (dot-sourced).

Tests: `powershell -NoProfile -Command "Invoke-Pester tools/spikes/s3-vhid/tests"`.
