# 0016 — Virtual HID: production keyboard, mouse and gamepad with an IOCTL report feed

**Status:** in-progress
**Date:** 2026-10-26
**Roadmap:** T22
**Plan:** [0016-virtual-hid](../plans/0016-virtual-hid.md) (local only, written by writing-plans skill)
**Crates touched:** `drivers/vhid`, `crates/input`, `apps/svc`

## Goal
Turn the S3 spike driver (spec 0006) into the input device the host ships with (ADR-0007): only the
MixLink service can feed it, it no longer shows up as a second game controller, keys never stay stuck
when the feeder dies, and the gamepad layout is one Steam can bind fully. On the Rust side,
`mixlink-input` gets the host half: HID report builders and a feeder the service owns. T10 / T12 then
only have to map network events to these calls.

## Scope
- In:
  - `vhid`: feed through a custom device interface + IOCTLs (SYSTEM + Admins only), vendor collection
    removed; release-all on feeder close; NKRO keyboard; gamepad layout change (triggers); query IOCTL
    (wire version, dropped count).
  - `crates/input`: pure HID report builders (keyboard, mouse rel / abs, gamepad) with layout tests
    mirroring `drivers/vhid`; `platform/windows/vhid.rs` feeder (open, feed, query, close).
  - `apps/svc`: holds the feeder for its lifetime (like `vdisplay`); dev commands `input status`,
    `input try`, hidden `input probe` (latency).
- Out:
  - Network input events, Raw Input / pointer lock on the client, mapping events → reports: T10.
  - `SendInput` fallback code: T10. XInput / ViGEm path, pad order vs a real pad, SDL mapping: T12.
  - More than one gamepad, pads that come and go per session (Open questions).
  - Keyboard LEDs, rumble / force feedback, pen / touch.
  - Final VID/PID (Open questions); the sample IDs `DEED/FEED` stay for phase 1.

## Design

### Control channel (`drivers/vhid`)
The spike fed reports through a HID vendor collection (Col05). Two problems (spec 0006 → Verdict):
DirectInput and Steam list it as a second controller, and any process that can open it can inject input.
- A device interface on the HID devnode itself does not work: `vhid` is a lower filter below
  `mshidumdf.sys`, and hidclass refuses a create on its FDO (`ERROR_GEN_FAILURE`, Results → probe).
- **Control devnode:** a second root node `Root\MixLinkVHidCtl`, same package (`vhid.inf`, same
  `vhid.dll`), installed as a plain UMDF function driver (no hidclass). It exposes the interface (new
  MixLink GUID) with INF `Security` `D:P(A;;GA;;;SY)(A;;GA;;;BA)` (same as `vdisplay`); IOCTLs go to its
  default queue. Both nodes carry the same WUDF `DeviceGroupId`, so they share one WUDFHost process
  (UMDF device pooling); the HID device registers its context in a process-global the control device uses.
  The IOCTL thread completes the pending HID read itself: no extra wake, no shared memory.
- Not paired (the HID node is gone, or Windows put the nodes in two processes): `QUERY` / `FEED` fail
  with `STATUS_DEVICE_NOT_CONNECTED`, `input status` says so.
- `drivers install | uninstall` create and remove both nodes (`DriverSpec` lists both hardware IDs).
- Rejected fallbacks: shared section + event (extra wake, release-all needs a heartbeat); keeping Col05
  with an ACL (second controller stays, no release-all).
- Col05 and its output / feature reports are removed from the descriptor.

| IOCTL | In | Out | Driver does |
|---|---|---|---|
| `FEED` | version, target report ID, payload (≤ `PAYLOAD_MAX`) | — | complete next pending read, else ring (64), else drop + count |
| `QUERY` | — | wire version, dropped count, report sizes | read state |

- Strict checks: version 1, known target, exact payload length for that target → else
  `STATUS_INVALID_PARAMETER`. Wire structs in `drivers/vhid/Control.h`, mirrored in
  `crates/input/src/platform/windows/vhid_wire.rs` with size / offset tests.
- **Release-all:** on cleanup of the file object that fed reports, the driver queues a neutral report
  per collection (no keys, no buttons, sticks centred, triggers 0; absolute mouse is skipped so the
  cursor does not jump). A crashed or killed service
  therefore never leaves a key or button held.
- One feeder at a time: the first file object that feeds owns the device until it closes; `FEED` from
  another gets `STATUS_SHARING_VIOLATION`, so a dev command cannot fight the running service. `QUERY`
  works from any opener and reports whether a feeder holds the device.
- HOT PATH: no allocation, no logging. The control queue and the HID queues run on different threads, so
  ring, pending reads and owner sit behind the HID device's framework lock (uncontended except at cleanup).

### Report layouts (`Descriptor.c`, mirrored in `crates/input/src/hid/`)
| ID | Collection | Payload | Change vs spike |
|---|---|---|---|
| 1 | Keyboard | modifiers (8 bits) + bitmap of usages 0x04..0x73 and 0x87..0x8B (international) | NKRO instead of 6-key array |
| 2 | Mouse relative | 5 buttons, X/Y i16, wheel i8, AC Pan i8 | none |
| 3 | Mouse absolute | 5 buttons, X/Y 0..32767 | none |
| 4 | Gamepad | 16 buttons, hat, sticks X/Y/Rx/Ry 16-bit, triggers 16-bit | triggers: see below |

- Triggers: Steam's setup wizard ignored the 8-bit Z / Rz triggers (spec 0006 → Results). Layout A:
  triggers as Simulation page Brake / Accelerator (what the Xbox Bluetooth HID descriptor uses), 10-bit.
  If Steam still ignores them, layout B: 16-bit Z / Rz. The acceptance run picks one; the other is
  dropped from the code.
- Keyboard bitmap: 8 modifier + 112 + 5 bits → payload 16 B, `PAYLOAD_MAX` grows to fit. Windows' `kbdhid` reads
  bitmap keyboards; the probe task checks keys still arrive in Raw Input and on the lock screen.

### Host side (`crates/input`)
- `hid/keyboard.rs`: `KeyboardState` (press / release by HID usage → report bytes; unknown usage ignored).
- `hid/mouse.rs`: `mouse_rel_report(buttons, dx, dy, wheel, pan)` splits deltas beyond i16 into several
  reports; `mouse_abs_report(buttons, x, y)` with x / y already in 0..32767.
- `hid/gamepad.rs`: `PadState` (Xbox-style: buttons bitset, d-pad, sticks i16, triggers u16) →
  report (stick i16 → 0..65535, trigger scaled to the layout's range).
- `hid/report.rs`: `Report` (target ID + fixed `[u8; PAYLOAD_MAX]` + len), no heap. All pure, unit tested.
- `platform/windows/vhid.rs`: `VHid::open()` (finds the interface with `CM_Get_Device_Interface_List`),
  `feed(&Report)` (`DeviceIoControl`, synchronous), `query()`, close on `Drop`. Trait `ReportSink` so
  callers test with a fake. `unsafe` only here.

### Service (`apps/svc`)
- `run.rs` opens `VHid` at start and keeps it, like `vdisplay`; missing driver = a warning in the log; `input status` shows it
  (T10 adds the `SendInput` fallback). Closing it on stop releases
  everything (driver release-all). `drivers install | uninstall` already stop the service first.
- `input status`: driver present, wire version, dropped count, who holds the feeder (open fails with
  sharing violation → "the MixLink service has it").
- `input try --script keys|mouse|abs|pad|all [--delay <s>]` (admin, service stopped): side-effect-free
  scripted input like `s3-feed` (F13–F24 keys, +d / −d mouse pairs, pad sweep); `--delay` lets the user
  focus a game, the UAC prompt or the lock screen first. Since the driver injects as hardware, the
  caller's session does not matter.
- `input probe --device kbd|rel|abs|pad [--count N]` (hidden, admin, elevated receiver): `s3-latency`
  method (QPC before `FEED`, Raw Input `RIDEV_INPUTSINK` on a message-only window, matching after the
  run), prints p50 / p99 / max and lost. Pad also via WGI if cheap; otherwise Raw Input only.

## Latency impact
Host injection stage only (feed call → input visible to apps), budget ≤ 2 ms client event → host.
`DeviceIoControl` on the control node instead of `WriteFile` on a HID collection; the IOCTL thread completes
the HID read directly, plus one uncontended framework lock. Expected the same as spec 0006 (kbd p99 0.8 ms,
abs 0.6 ms, pad via WGI 0.5 ms; relative mouse ~3.3 ms is Windows'). Measured with `input probe`, elevated,
release build.

## Acceptance criteria
- [ ] Only SYSTEM and Administrators can open the feed; a normal user's open fails (access denied).
- [ ] DirectInput, joy.cpl and Steam list exactly one MixLink controller; Device Manager shows no
  vendor-defined MixLink device.
- [ ] `input try --script all` moves the cursor, types F13–F24 and moves the pad (joy.cpl), no event
  flagged injected (LL hook check as spec 0006 case C).
- [ ] Holding a key / button with `input try`, then killing the process: the key is released at once
  (no auto-repeat continues; Raw Input sees the release).
- [ ] A second feeder while the service runs gets a clear "in use by the MixLink service" message.
- [ ] Keys and mouse reach the UAC prompt and the lock screen (`input try --delay`; Tab / Space only).
- [ ] Keyboard: 10 keys held at once all arrive (NKRO); one game from spec 0006 still walks / looks.
- [ ] Steam's setup wizard binds LT / RT as analog axes; one game from spec 0006 reacts to the pad
  (Steam Input on), incl. triggers.
- [ ] `input probe`, 10 000 events each, 0 lost: kbd, abs and pad (WGI or Raw Input) p99 ≤ 1 ms;
  relative mouse recorded (Windows tail, spec 0006).
- [ ] `drivers uninstall` with the service holding the feeder removes `vhid` cleanly (service stopped
  first); with Steam holding the pad: exit 3 and the program named (spec 0014 open row).

## Results
**Machine:** main PC (spec 0006 machine), Windows 11 Pro 26H2, Secure Boot on, test-signing off.

| Check | Observed |
|---|---|
| Probe: interface on the HID devnode (2026-10-26) | **Fails.** Admin open → `0x8007001F` (`ERROR_GEN_FAILURE`, hidclass refuses the create on its FDO); normal user → access denied (INF `Security` applies). User chose the control devnode (Design). |
| Control node paired (2026-10-26) | **Pass.** `input status` (admin): wire version 1, sizes 8 / 7 / 5 / 13, feeder free: the control node reaches the HID node in one WUDFHost. Normal user: access denied. Steam still lists 2 controllers (Col05 is removed later in this spec). |

## Open questions
- Gamepads: one pad always present (this spec), or later per-session pads (the service adds / removes a
  `Root\MixLinkVHid` node, so a pad appears only while a client pad is connected, and up to 4)? Needs a
  roadmap row; affects pad order vs a real pad (T12).
- VID/PID for release: the `DEED/FEED` test IDs stay for phase 1; a pid.codes allocation (project is
  MIT / Apache-2.0) before phase 2.
