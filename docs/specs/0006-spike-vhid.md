# 0006 — Spike S3: UMDF virtual HID keyboard, mouse and gamepad

**Status:** done
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
- [x] A: `vhid` signed per machine installs and loads (Secure Boot on, test-signing off) and survives reboot.
- [x] B: Windows lists the keyboard, both mice and the game controller; `s3-apis` table in Results.
- [x] C: events from `vhid` are not flagged injected (`sendinput` control is).
- [ ] D: p99 ≤ 1 ms and 0 lost for HID keyboard, mouse relative, mouse absolute and gamepad. **Partly:** 0 lost
  everywhere; keyboard, mouse absolute, gamepad via WGI pass; mouse relative ~3.3 ms and gamepad via Raw Input
  ~1.1 ms fail, with `SendInput` relative mouse at ~3.0 ms too (Windows path, not the driver).
- [x] E: `sendinput` and ViGEm latencies recorded; ViGEm p99 ≤ 1 ms (58 µs).
- [x] F, G: input fed as LocalSystem from Session 0 reaches the UAC prompt and the lock screen.
- [x] H: keyboard and mouse work in all three games.
- [x] I–K: per-game table for the three pad paths, incl. whether Steam needed a manual mapping.
- [x] Results filled (cases table with Expected / Observed / Log) and the verdict: ADR-0007 holds,
  holds with changes (named), or keyboard/mouse fall back to `SendInput` (ADR-0004 fallback).

## Results
**Machine:** Windows 11 Pro 26H2 build 26300.9711, Secure Boot on, HVCI on, SAC off, test-signing off.
Same PC as S1/S2 (RTX 5070). Driver signed and installed with `install.ps1` (pnputil 0, devcon 0).
Logs: `tools/spikes/s3-vhid/out/` (local only). Latency runs: release build, 10 000 events, 2 ms apart,
receiver run **elevated** (see Measurement notes).

| Case | Expected | Observed | Log |
|---|---|---|---|
| A | loads, problem code 0, survives reboot | **Pass.** `ROOT\HIDCLASS\0000` OK, code 0 before and after reboot; catalog `Valid`, signer `CN=MixLink Input Driver (this PC)`, private key removed | `20261023-031512-A-install`, `-031720-A-reboot-report` |
| B | kbd, 2 mice, pad listed | **Pass.** Children: HID Keyboard (Col01), 2× HID-compliant mouse (Col02/03), game controller (Col04), vendor device (Col05). APIs: table below | `1792730585-apis` |
| C | no injected flag | **Pass.** LL hooks: 600 + 800 `vhid` keys, 0 injected; `SendInput` control 50/50 injected | hook runs (scratch) |
| D | p99 ≤ 1 ms, 0 lost | **Partly.** Latency table below; 0 lost and 0 driver drops in every run | `17927306xx`–`1792731160` |
| E | recorded; ViGEm ≤ 1 ms | **Pass.** Table below | `1792730683`, `-0901`, `1792745006` |
| F | UAC focus moves | **Pass.** Mouse moved, Tab ×3 moved focus in the UAC prompt (task result 0) | `20261023-120058-UacTab` |
| G | lock screen reacts | **Pass.** Lock screen lifted to the sign-in box (task result 0) | `20261023-120422-LockSpace` |
| H | games react to kbd + mouse | **Pass.** Walk / Look / Menu in Elden Ring, Clair Obscur: Expedition 33, Wuchang | user |
| I | pad via Steam Input | **Pass** with a manual layout (game matrix below) | user |
| J | pad, Steam Input off | **As expected:** no game reacts (XInput-only) | user |
| K | ViGEm, Steam Input off | **Pass** in Wuchang; Elden Ring not usable as a test (a real Xbox pad fails there too) | user |

| Path (p50 / p99 / max) | Run 1 | Run 2 |
|---|---|---|
| `vhid` keyboard → Raw Input | 0.20 / **0.80** / 7.0 ms | — |
| `vhid` mouse relative → Raw Input | 0.26 / **3.36** / 25.5 ms | 0.23 / **3.25** / 17.0 ms |
| `vhid` mouse absolute → Raw Input | 0.15 / **0.62** / 6.6 ms | — |
| `vhid` gamepad → Raw Input | 0.40 / **1.15** / 12.6 ms | 0.38 / **1.10** / 13.3 ms |
| `vhid` gamepad → WGI `RawGameController` (polled) | 0.06 / **0.54** / 13.0 ms | — |
| `SendInput` keyboard → Raw Input | 0.13 / **0.36** / 1.3 ms | — |
| `SendInput` mouse relative → Raw Input | 0.08 / **2.98** / 5.8 ms | — |
| ViGEm pad → XInput (polled; 14 / 10 000 states skipped) | 0.013 / **0.058** / 0.29 ms | — |

| API (`s3-apis`) | Sees |
|---|---|
| Raw Input | Col01 keyboard, Col02 + Col03 mouse, Col04 pad (Col05 vendor is not listed) |
| DirectInput 8 | "MixLink Virtual Input" **twice** (pad + vendor control collection), joy.cpl shows one |
| WGI | `RawGameController` (6 axes, 16 buttons, 1 switch); `Gamepad` list empty |
| XInput | nothing (`vhid` is not an XUSB device); ViGEm pad at index 0 |

| Game | I: `vhid` pad + Steam Input | J: `vhid` pad, Steam Input off | K: ViGEm, Steam Input off |
|---|---|---|---|
| Elden Ring (offline) | Pass 3/3 launches | No reaction | No movement; **real Xbox pad also fails** → inconclusive |
| Clair Obscur: Expedition 33 | Pass once Steam Input is enabled for the game (default was off: 0/4 before) | No reaction | not run |
| Wuchang: Fallen Feathers | Pass 2/3 launches (1 no reaction, cause not found) | No reaction | **Pass** (real Xbox pad control also works) |

Steam lists the pad twice and needs "Begin Setup". Buttons, D-pad and sticks map; **LT / RT analog axes
(Z / Rz, 8-bit) are ignored by the wizard** even after priming and a 2 s hold, though joy.cpl shows them
move; bound as spare buttons `b13` / `b14` instead. The wizard binds whatever input arrives first.

Measurement notes:
- **UIPI.** A non-elevated receiver (Raw Input sink, LL hook) gets nothing while an elevated window is in the
  foreground (100 / 0 / 100 with normal / admin / normal focus); the input still reaches the elevated
  window. Runs before this was found are discarded.
- **Arrival matching.** The first harness unwrapped sequence numbers online; a stale first reading (XInput)
  shifted every index (ViGEm 0 / 10 000). Fixed in `e1b8c49`: arrivals are matched to send times after the run.
  The earlier `vhid` 10 000-event runs all received 10 000 / 10 000, so their numbers stand.
- An LL hook in the loop adds ~1 ms to Raw Input p50 (kbd 0.16 → 1.15 ms); hooks were off for timing runs.
- Driver path itself is fast: WGI sees the pad report 59 µs (p50) after `WriteFile`. Raw Input and the
  relative-mouse path add the rest, for `SendInput` as well.

### Verdict: ADR-0007 holds, with changes
A per-machine signed UMDF HID minidriver gives Windows a keyboard, two mice and a gamepad that are treated
as hardware: not flagged injected, delivered to the UAC secure desktop and the lock screen from Session 0,
and used by all three games (keyboard / mouse directly, pad through Steam Input). Non-Steam XInput games
need the ViGEm path, which works (Wuchang) as ADR-0007 planned. Latency: 0 lost; keyboard, absolute mouse
and WGI pad within 1 ms p99; relative mouse and the Raw Input pad path exceed it by the same margin
`SendInput` does, so the cost is Windows', not the driver's.

Changes carried forward:
- **T22:** move the feed off the HID vendor collection to a custom device interface (IOCTL) on the UMDF
  device: it shows up as a second game controller (DirectInput, Steam) and any process can open it.
- **T12 / T22:** triggers as Steam reads them (16-bit or Xbox-HID-like layout, or an SDL mapping); a real pad
  on the host takes player 1, so stream sessions must control pad order; games may ship with Steam
  Input off by default (E33).
- **T20:** uninstall: `pnputil /remove-device` and `/disable-device` refuse the `vhid` node ("critical
  device": it hosts a keyboard); `devcon remove @<instance>` removes it at once, no reboot
  (`uninstall.ps1` falls back to it; log `20261023-163242-cleanup2-uninstall`: clean).
- **T10 / phase 3:** budget the Raw Input / relative-mouse tail (~3 ms p99) in the input path; measure as
  elevated.

## Open questions
- VID/PID for the shipped device: the sample's test IDs are fine for a spike; production needs IDs we may
  use (e.g. a pid.codes allocation). Decide in T22.
- Who may open the control collection: any process that can open it can inject input. T22 decides how
  to restrict it to the service (custom device interface on the UMDF device instead of a HID collection).
- (Answered 2026-10-23: yes, "Begin Setup" per pad, and the analog triggers are ignored.) Steam generic gamepad support may need a one-time manual mapping per pad; if so, T12 checks whether
  shipping an SDL mapping string (or a fixed descriptor layout Steam already knows) avoids it.
