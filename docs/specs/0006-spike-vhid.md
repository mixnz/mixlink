# 0006 — Spike S3: UMDF virtual HID keyboard, mouse and gamepad

**Status:** in-progress
**Date:** 2026-10-23
**Roadmap:** T16
**Plan:** [0006-spike-vhid](../plans/0006-spike-vhid.md) (local only, written by writing-plans skill)
**Crates touched:** none. New: `drivers/vhid/`, `tools/spikes/s3-vhid/`. Changed: `drivers/build.ps1`

## Goal
Prove or disprove ADR-0007: a UMDF HID minidriver, signed per machine (ADR-0004), exposes a keyboard,
a mouse and a gamepad that Windows and real games treat as hardware, fed from user mode fast enough for
the input budget (client event → injected on host ≤ 2 ms, `latency-budget.md`). Also measure the
gamepad paths games actually read: Steam Input over our HID pad, and ViGEmBus (XInput) for non-Steam
games. T18 and T22 wait on this answer.

## Scope
- In:
  - `drivers/vhid/`: Microsoft `vhidmini2` sample (UMDF 2 build, MIT), renamed like `vdisplay`
    (hardware ID `Root\MixLinkVHid`, files `vhid.inf/.dll/.cat`). One device, four top-level collections:
    keyboard, mouse, gamepad, vendor-defined control (Design).
  - `drivers/build.ps1`: builds `vhid` as well (`-Driver vdisplay|vhid|all`), same output layout
    `target/drivers/<name>/pkg/`.
  - `tools/spikes/s3-vhid/`: standalone Cargo package (own `[workspace]`, throwaway), binaries:
    - `s3-feed`: sends scripted input through one backend: `vhid`, `vigem` or `sendinput`;
    - `s3-latency`: feeder + receiver in one process, timing runs (Design);
    - `s3-apis`: lists which APIs see which device (Raw Input, DirectInput, Windows.Gaming.Input, XInput);
    - `s3-hook`: low-level keyboard/mouse hooks, logs the `INJECTED` flags of every event.
  - Scripts (user runs them elevated): `install.ps1` / `uninstall.ps1` for `vhid` (reuse S1's
    `sign-catalog.ps1`), `run-system.ps1` (runs `s3-feed` as LocalSystem in Session 0 via a one-shot
    scheduled task), `report.ps1` (device tree, driver state, HID collections).
  - Game checks by the user: Elden Ring (offline mode), Clair Obscur: Expedition 33, Wuchang: Fallen Feathers.
  - Results written into this spec in S1's format, and the verdict for ADR-0007.
- Out:
  - Production `vhid` (IOCTL report feed contract, per-client devices, multiple pads), `mixlink-input`
    HID report builder, `SendInput` fallback code: T22, T10, T12.
  - Install tooling, certificate lifecycle, WDK build in gate/CI: T20.
  - Anti-cheat titles with anti-cheat on (Elden Ring online, Vanguard, EAC, BattlEye, FACEIT): T18.
  - Emulating a Sony or Xbox VID/PID (ADR-0007). Rumble / force feedback, keyboard LEDs.
  - Shipping ViGEmBus: the user installs the author-signed release by hand for this spike.

## Design

### Driver (`vhid`, WUDFHost, mshidumdf.sys above it)
- Report descriptor, one report ID per collection:
  - **Keyboard** (Generic Desktop / Keyboard): modifier byte + 6-key array (boot layout, 6KRO; NKRO is
    a T22 choice, not a spike risk).
  - **Mouse relative** (Generic Desktop / Mouse): 5 buttons, X/Y 16-bit relative, wheel, horizontal wheel.
  - **Mouse absolute** (Generic Desktop / Mouse, second collection): 5 buttons, X/Y 0..32767 absolute.
  - **Gamepad** (Generic Desktop / Game Pad): 16 buttons, hat switch (D-pad), 4 axes (two sticks, 16-bit),
    2 triggers (Z/Rz, 8-bit). Layout mirrors an Xbox pad so Steam's mapping is one-to-one.
  - **Control** (vendor page 0xFF00): output report = target report ID + payload.
- Feed path: `s3-*` opens the control collection and calls `WriteFile` (output report). The driver
  completes the next pending `IOCTL_HID_READ_REPORT` with an input report carrying the target report ID;
  hidclass routes it to that collection. No timer: input is delivered when it is written (the sample's
  timer-driven read is removed).
- Pending reads are per device (hidclass keeps a few outstanding) in one manual queue. When none is
  pending, the report goes into a preallocated ring (64 entries) that the next read drains; ring full =
  drop and count. No allocation per report.
- Control collection access: hidclass's default for HID collections (child PDOs, not set by our INF).
  The spike records who can open it; restricting it to SYSTEM is a T22 question.
- VID/PID: the sample's test values, renamed product string "MixLink Virtual Input". Real IDs: Open questions.

### ViGEmBus path (XInput)
- `s3-feed vigem` / `s3-latency vigem`: create one virtual Xbox 360 pad through ViGEmBus and send the same
  scripted gamepad state. Client library: a Rust ViGEmClient binding (crate chosen and verified in the plan).
- ViGEmBus is installed by the user from its author's signed release; scripts check it is present and
  never download anything.

### Latency measurement (QPC, one process, one machine)
- `s3-latency <backend> <device>`: thread A writes N = 10 000 events, 2 ms apart, each carrying a sequence
  number in a field the receiver can read back. QPC before the write call. Side-effect free by choice:
  keys cycle F13–F24 (press / release), mouse deltas come in +d / −d pairs so the cursor returns, the pad
  encodes the sequence in an axis value. Thread B receives and takes QPC on arrival. Arrays preallocated, stats after.
  - Keyboard / mouse / HID gamepad: Raw Input (`RIDEV_INPUTSINK`), QPC when `WM_INPUT` is read.
  - XInput (ViGEm) and Windows.Gaming.Input: busy-poll on a dedicated thread, QPC on first changed state.
  - `sendinput` backend (keyboard, mouse): same receiver, for comparison.
- Report p50 / p99 / max per backend × device, and lost events (sequence gaps).
- Feeder runs in the user session for timing runs; the SYSTEM run (`run-system.ps1`) is functional only.

### Test cases
| Case | What | Expected if ADR-0007 holds |
|---|---|---|
| A. Install | sign + install `vhid` with Secure Boot on, test-signing off; reboot | installs, loads, problem code 0; still there after reboot |
| B. Devices | Device Manager + `s3-apis` | one keyboard, two mice, one game controller; Raw Input sees all; DirectInput and WGI (`RawGameController`) see the pad; XInput does not |
| C. Injected flag | `s3-hook` while `s3-feed vhid` types and moves | no event flagged injected; `sendinput` run flagged (control) |
| D. Latency HID | `s3-latency vhid` keyboard, mouse rel, mouse abs, gamepad | p99 ≤ 1 ms each; 0 lost |
| E. Latency compare | `s3-latency sendinput` (kbd, mouse), `s3-latency vigem` (pad via XInput) | recorded; ViGEm p99 ≤ 1 ms |
| F. UAC | UAC prompt open, `run-system.ps1` moves the mouse and presses Tab (never Enter) | focus moves in the prompt |
| G. Lock screen | Win+L, `run-system.ps1` moves the mouse and presses Space | lock screen reacts (PIN box shows); no credentials typed |
| H. Games kbd+mouse | 3 games, `s3-feed vhid` scripted walk / camera / menu | each game reacts to keys and mouse look |
| I. Games pad, Steam Input | 3 games via Steam, Steam Input on for the HID pad (generic pad support) | Steam sees the pad (note if mapping was needed); each game reacts |
| J. Games pad, no Steam Input | Steam Input off for the game | record per game: reacts or not (expected: not, XInput-only) |
| K. Games pad, ViGEm | Steam Input off, `s3-feed vigem` | each game reacts |

Scripted input for games (H–K): `s3-feed <backend> <scenario> --delay <s>` starts after a countdown so the
user can focus the game; scenarios are short (walk forward, turn camera, open menu, confirm). Real devices
stay connected and untouched. Scripts change drivers on the machine, so **the user runs them**; the AI
session builds, reads logs and fills Results. Logs: `tools/spikes/s3-vhid/out/` (gitignored).

### Machine
Same as S1/S2: Win11 26H2, Secure Boot on, HVCI as found (recorded). Before A: the S2b driver, service
`MixLinkS2` and registry key are removed (spec 0005 → Verdict clean-up), physical monitor main again.

## Latency impact
Measures the host injection stage only (feed call → input visible to apps). Target p99 ≤ 1 ms, half of the
2 ms input budget, leaving the rest for network and the service. No production hot path changes.

## Acceptance criteria
- [ ] A: `vhid` signed per machine installs and loads (Secure Boot on, test-signing off) and survives reboot.
- [ ] B: Windows lists the keyboard, both mice and the game controller; `s3-apis` table in Results.
- [ ] C: events from `vhid` are not flagged injected (`sendinput` control is).
- [ ] D: p99 ≤ 1 ms and 0 lost for HID keyboard, mouse relative, mouse absolute and gamepad.
- [ ] E: `sendinput` and ViGEm latencies recorded; ViGEm p99 ≤ 1 ms.
- [ ] F, G: input fed as LocalSystem from Session 0 reaches the UAC prompt and the lock screen.
- [ ] H: keyboard and mouse work in all three games.
- [ ] I–K: per-game table for the three pad paths, incl. whether Steam needed a manual mapping.
- [ ] Results filled (cases table with Expected / Observed / Log) and the verdict: ADR-0007 holds,
  holds with changes (named), or keyboard/mouse fall back to `SendInput` (ADR-0004 fallback).

## Results
(filled after the runs)

## Open questions
- VID/PID for the shipped device: the sample's test IDs are fine for a spike; production needs IDs we may
  use (e.g. a pid.codes allocation). Decide in T22.
- Who may open the control collection: any process that can open it can inject input. T22 decides how
  to restrict it to the service (custom device interface on the UMDF device instead of a HID collection).
- Steam generic gamepad support may need a one-time manual mapping per pad; if so, T12 checks whether
  shipping an SDL mapping string (or a fixed descriptor layout Steam already knows) avoids it.
