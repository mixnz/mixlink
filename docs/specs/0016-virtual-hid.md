# 0016 — Virtual HID: keyboard, mouse and virtual Xbox pads with an IOCTL report feed

**Status:** in-progress
**Date:** 2026-10-26
**Roadmap:** T22
**Plan:** [0016-virtual-hid](../plans/0016-virtual-hid.md) (local only, written by writing-plans skill)
**Crates touched:** `drivers/vhid`, `crates/input`, `crates/core`, `apps/svc`

## Goal
Turn the S3 spike driver (spec 0006) into the input devices the host ships with: only the MixLink
service can feed them, keys never stay stuck when the feeder dies, and every client gamepad shows up on
the host as an **Xbox controller** that Windows (XInput), Steam and games recognise with no setup step.
The service adds a pad when a client pad arrives and removes it when it leaves (up to 4, split screen).
On the Rust side, `mixlink-input` gets the host half: report builders and a feeder the service owns.
T10 / T12 then only map network events to these calls. ADR-0009 (new) replaces ADR-0007's gamepad part.

## Scope
- In:
  - `vhid` keyboard / mouse node: feed through a control node + IOCTLs (SYSTEM + Admins only), vendor
    collection removed, release-all on feeder close, NKRO keyboard, query IOCTL.
  - `vhid` pad nodes: one root node per pad (slots 0..3), HID identity of an Xbox Series controller
    over Bluetooth (`045E:0B13`), bound by the inbox `xinputhid` filter (XInput). Output (rumble) reports
    are accepted and kept, not forwarded.
  - `apps/svc`: add / remove pad nodes; holds the feeder; dev commands `input status | try | pad | probe`
    and `input pad-add | pad-remove`.
  - `crates/input`: pure report builders (keyboard, mouse rel / abs, Xbox pad), feeder `VHid`.
  - `crates/core`: `device_io` (interface open + IOCTL) shared by `vdisplay` and `vhid`.
  - ADR-0009: host pads are virtual Xbox controllers through `xinputhid`; ViGEm only as fallback.
- Out:
  - Network input events, client capture, mapping events → reports, split-screen session wiring: T10 / T12.
  - Rumble / LED forwarding to the client: T12. Other personas (DualShock 4, DualSense, Switch Pro): later row.
  - `SendInput` fallback code: T10. Keyboard LEDs, pen / touch.
  - Final VID/PID of the keyboard / mouse node (`DEED/FEED` test IDs stay for phase 1).

## Design

### Control channel (`drivers/vhid`)
The spike fed reports through a HID vendor collection (Col05): DirectInput and Steam listed it as a second
controller, and any process that could open it could inject input (spec 0006 → Verdict).
- A device interface on a HID node does not work: `vhid` is a lower filter below `mshidumdf.sys`, and
  hidclass refuses a create on its FDO (`ERROR_GEN_FAILURE`, Results).
- **Control node** `Root\MixLinkVHidCtl`: same package (`vhid.inf`, `vhid.dll`), plain UMDF function
  driver. It exposes the MixLink interface with INF `Security` `D:P(A;;GA;;;SY)(A;;GA;;;BA)`. All `vhid`
  nodes carry WUDF `DeviceGroupId` `MixLinkVHidGroup`, so they share one WUDFHost; each HID node registers
  its context in a process-global table the control node feeds. The IOCTL thread completes the pending
  HID read itself: no extra wake, no shared memory. One `SRWLOCK` guards the table, rings, pending reads,
  owner and last reports (uncontended except at cleanup).
- Not paired (target node gone, or nodes in two processes): `FEED` / `QUERY` fail
  `STATUS_DEVICE_NOT_CONNECTED`; `input status` says so.

| IOCTL | In | Out | Driver does |
|---|---|---|---|
| `FEED` | version, target, payload | — | target node: complete next pending read, else ring (64), else drop + count |
| `QUERY` | version | wire version, dropped, feeder held, pads present (bit per slot), report sizes | read state |

- Targets: 1 keyboard, 2 mouse relative, 3 mouse absolute (keyboard / mouse node); 0x10 + slot = pad
  input report 0x01 of pad slot 0..3. Strict checks: version, known target, exact payload length, zero
  padding → else `STATUS_INVALID_PARAMETER`; target pad absent → `STATUS_DEVICE_NOT_CONNECTED`.
- **Release-all:** cleanup of the feeding file object queues a neutral report per collection on every
  node (no keys, no buttons, sticks centred, triggers 0; absolute mouse skipped so the cursor does not jump).
- One feeder at a time: the first file object that feeds owns all nodes until it closes; `FEED` from another
  gets `STATUS_SHARING_VIOLATION`. `QUERY` works from any opener.
- **Get Input Report:** every HID node answers `IOCTL_UMDF_HID_GET_INPUT_REPORT` with the last report fed
  (neutral at start). Readers that open a device read where each axis rests (Results: Steam).
- HOT PATH: no allocation, no logging, one exclusive SRW acquire per report.

### Keyboard / mouse node (`Root\MixLinkVHid`, `Descriptor.c`)
| ID | Collection | Payload |
|---|---|---|
| 1 | Keyboard | modifiers + bitmap of usages 0x04..0x73 and 0x87..0x8B (NKRO), 16 B |
| 2 | Mouse relative | 5 buttons, X/Y i16, wheel i8, AC Pan i8, 7 B |
| 3 | Mouse absolute | 5 buttons, X/Y 0..32767, 5 B |

The generic gamepad collection (ID 4) is removed: generic pads need Steam's setup wizard, which binds
triggers unreliably (Results), and XInput-only games never see them.

### Virtual Xbox pads (`xinputhid`)
- Inbox `xinputhid.inf` (Microsoft-signed) binds its `GIP_Hid` section to the HID collection hardware ID
  `HID\VID_045E&PID_0B13&IG_00` and adds `xinputhid` as upper filter: the pad becomes an XInput device and
  a Windows.Gaming.Input `Gamepad`. No kernel driver of ours (ADR-0004 holds).
- A HID child takes its enumerator from its parent's instance path, so a pad node is created with an
  explicit instance ID whose enumerator carries `VID_045E&PID_0B13&IG_00` (prior art: HIDMaestro, see
  Open questions). Its hardware ID `Root\MixLinkXboxPad` selects the pad install section of `vhid.inf`;
  if the probe shows WGI missing or doubling the pad, the pad section also writes `xinputhid` to the
  node's `UpperFilters` (HIDMaestro's WGI admission).
- **First plan task probes this** (instance naming, xinputhid binding, XInput slot, Steam). If xinputhid
  will not bind, stop and ask: fallback is ViGEm (X360) for every pad, already an optional component.
- Driver role from the hardware ID; HID attributes VID `045E`, PID `0B13`; one top-level collection (no
  `&Col0x` suffix, or the hardware ID would not match). Descriptor = the Xbox Series Bluetooth descriptor
  (input report 0x01, guide in 0x02, rumble output 0x03), captured from a real controller in the plan.
- Input report 0x01 (16-byte layout SDL parses for Bluetooth): LX, LY, RX, RY u16 (centre 0x8000),
  LT, RT 10-bit in u16, hat (1..8 clockwise from up, 0 = centred), buttons A B X Y LB RB View Menu
  (byte 14), LS RS (byte 15). Exact bytes follow the captured descriptor.
- Rumble report 0x03 (`{03, mask, lt, rt, low, high, duration, delay, repeat}`) is accepted, its last
  value kept per pad for T12, never fails the writer.
- Each pad node registers in the process-global table by slot (from its instance ID). Container GUID per
  slot (not the null sentinel, which makes XInput skip a slot: HIDMaestro note).
- Service `PadNodes`: `add(slot)` creates the node (SetupAPI, explicit instance ID, driver already in the
  store) and waits until the pad registers; `remove(slot)` removes it (`CM_Query_And_Remove_SubTree`).
  `drivers install | uninstall` remove leftover pad nodes. Service start removes pad nodes left by a crash.

### Host side (`crates/input`)
- `hid/keyboard.rs`, `hid/mouse.rs`, `hid/report.rs` as built (NKRO, split relative deltas, no heap).
- `hid/xbox.rs`: `PadState` (buttons, d-pad, sticks i16, triggers u16) → input report 0x01 bytes.
- `platform/windows/vhid.rs`: `VHid::open` (checks wire version and sizes), `feed`, `query`, `ReportSink`.

### Service (`apps/svc`)
- `run.rs` opens `VHid` and owns it (feeds neutral reports at once); missing driver = warning.
- `input status`, `input try --script keys|chord|mouse|abs|pad|all|hold|uac|lock|walk [--delay <s>]`,
  `input pad [--slot n] [--delay <s>]` (type `a`, `lt 500`, ...: axes ramp over 10 reports like a real
  pull), `input pad-add --slot n` / `pad-remove --slot n`, hidden `input probe --device kbd|rel|abs|pad`.

## Latency impact
Host injection stage only (feed call → input visible to apps), budget ≤ 2 ms client event → host.
Same driver path as spec 0006 plus one uncontended SRW lock: expected kbd p99 0.8 ms, abs 0.6 ms;
relative mouse ~3.3 ms is Windows'. Pad: `xinputhid` adds one kernel filter hop, measured with XInput
polling (`input probe --device pad`). Release build, elevated.

## Acceptance criteria
- [ ] Only SYSTEM and Administrators can open the feed; a normal user's open fails (access denied).
- [ ] Device Manager shows no vendor-defined MixLink device; with no pad added, no MixLink controller exists.
- [ ] `input try --script all` moves the cursor, types F13–F24, no event flagged injected (LL hook).
- [ ] Holding a key / pad button, then killing the feeder: released at once.
- [ ] A second feeder while the service runs gets "in use by the MixLink service".
- [ ] Keys and mouse reach the UAC prompt and the lock screen (`uac`, `lock` scripts).
- [ ] Keyboard: 10 keys held at once all arrive (NKRO); one game from spec 0006 walks / looks.
- [ ] `pad-add --slot 0`: Windows shows an Xbox controller with `xinputhid`; XInput reads slot 0; WGI lists
  one `Gamepad`; Steam shows an Xbox controller **without "Begin Setup"**, LT / RT 0 → max.
- [ ] Wuchang (XInput-only): with Steam Input off, RT stab and held RT charged attack work.
- [ ] `pad-add` slots 0 and 1: XInput sees two pads (split screen); `pad-remove --slot 1` removes one.
- [ ] `input probe`, 10 000 events, 0 lost: kbd, abs, pad (XInput) p99 ≤ 1 ms; relative mouse recorded.
- [ ] `drivers uninstall` with the service holding the feeder removes all `vhid` nodes cleanly.

## Results
**Machine:** main PC (spec 0006 machine), Windows 11 Pro 26H2, Secure Boot on, test-signing off.

| Check | Observed |
|---|---|
| Interface on the HID node | **Fails.** Admin open → `0x8007001F` (hidclass refuses the create on its FDO); normal user → access denied. User chose the control node. |
| Control node paired | **Pass.** `input status`: wire version 1, feeder free; normal user: access denied. |
| Generic pad in Steam | Steam reads only the **first axis INPUT item** of a generic HID pad: triggers in a second item never moved in Steam's test screen (Simulation Brake / Accelerator, Z / Rz, Rx / Ry all tried); one item for all six axes fixed that. Its setup wizard then needs ramped pulls (a one-report jump never binds) and assumes every axis rests at the centre, so a trigger resting at the minimum is split into two half-axis bindings (`lefttrigger:-a3`, `rightshoulder:+a3`), varying run to run. `SDL_GAMECONTROLLERCONFIG` is ignored by Steam. Verdict: generic pads dropped for virtual Xbox pads. |

## Open questions
- Instance naming that makes the HID child carry `&IG_00` from a root node (HIDMaestro uses
  `ROOT\VID_045E&PID_028E&IG_00\…` and, for `0B13`, an underscore form against a PnP edge case): the
  first plan task settles it.
- HIDMaestro (github.com/hifihedgehog/HIDMaestro) is read for ideas only; no code is copied.
- Microsoft VID/PID on a virtual device: accepted by the user for compatibility (as ViGEm, Sunshine,
  Steam do); the user-facing anti-cheat note (`docs/compat/anti-cheat.md`) gains a line.
