# mixlink-input

> Module card. Keep <= 50 lines. Update when the public API changes.

**Purpose:** Client: input state from keyboard (LL hook) and mouse (spec 0021). Host: apply client packets and build
HID reports for `drivers/vhid` (ADR-0007, ADR-0009, spec 0016). SendInput fallback: later row.
**Depends on:** mixlink-core (`device_io`, `input` state types).
**Hot path:** yes — every input event; send immediately, never batch per frame.

## Layout
- `hid/` (pure, no heap, tests in `hid_tests.rs`): `report.rs` (+ `ReportSink`), `keyboard.rs`, `mouse.rs`, `xbox.rs`;
  layouts = `drivers/vhid/Descriptor.c` (kbd / mouse) and `PadDescriptor.c` (Xbox pad).
- `keymap.rs` (scan code → HID usage), `client/state.rs` (`InputState`), `host/apply.rs` (`InputApplier`),
  `host/abs_map.rs` (`AbsMap`): pure, each with `<name>_tests.rs`.
- `platform/windows/vhid_wire.rs` (+ tests): IOCTL bytes = `drivers/vhid/Control.h`.
- `platform/windows/vhid.rs`: `VHid` feeder. FFI lives in `core::device_io`, so this file has no `unsafe`.

## Public API
- `hid::report`: `PAYLOAD_MAX` (16), `Target` (`Keyboard`, `MouseRel`, `MouseAbs`, `Pad(slot)`; `wire_id` 1 / 2 / 3 /
  0x10 + slot, `payload_len`), `Report` (`target`, `payload`), `neutral_reports()`, `ReportSink` (`type Error`, `feed`).
- `hid::keyboard`: `KeyboardState` (`set(usage, down) -> bool`, `clear`, `report`; NKRO).
- `hid::mouse`: `REL_MAX`, `mouse_rel_reports(buttons, dx, dy, wheel, pan) -> RelReports` (≤ 4, split),
  `mouse_abs_report(buttons, x, y)`.
- `hid::xbox`: `PAD_SLOTS` (4), `buttons::{A, B, X, Y, LB, RB, VIEW, MENU, GUIDE, LS, RS}`, `Dpad`, `PadState`
  (`NEUTRAL`, `report(slot)`; sticks i16 +y = down, triggers u16 → 10-bit, `share`).
- `keymap::usage(scan, extended, vk) -> Option<u8>` (set-1 scan code; Pause / NumLock by vk; AltGr's fake Ctrl dropped).
- `client::state::InputState` (`key`, `button`, `mouse_abs`, `mouse_rel`, `wheel`, `set_mode`, `release_all`, `held`,
  `packet` (next seq), `current` (same seq, repeats)).
- `host::abs_map`: `Rect`, `DesktopPlace { monitor, desktop }`, `AbsMap` (`new(stream, place)`, `map(x, y)`).
- `host::apply`: `InputApplier` (`new(map)`, `apply(packet, sink) -> Applied`, `release_all`, `held`, `counts`),
  `Applied` (`Stale`, `Fresh`), `ApplyCounts` (`packets`, `stale`, `recovered_events`, `bitmap_fixes`).
- `platform::windows::vhid_wire` (wire v2): `IOCTL_FEED`, `IOCTL_QUERY`, `WIRE_VERSION`, `QueryReply`
  (`pads_present`), `encode_feed`, `encode_query`, `decode_query`, `WireError`.
- `platform::windows::vhid`: `VHid::open` / `query` / `feed` (`ReportSink` for `VHid` and `&VHid`), `VHidError`
  (`Missing`, `AccessDenied`, `InUse`, `NotPaired`, `Wire`, `Win32`).

## Notes
- The feed goes to the control node `Root\MixLinkVHidCtl` (SYSTEM + Admins only), which shares a WUDFHost
  with the HID nodes; an interface on a HID node is refused by hidclass (spec 0016).
- Pads are separate software devices `SWD\MixLinkPad\000N` per slot (hardware ID
  `Root\VID_045E&PID_0B13&IG_00`, ADR-0009); the inbox `xinputhid` makes them XInput.
  A target whose pad has no node fails with `NotPaired`.
- Client buttons go on the relative mouse collection in every mode (press and release on one device); the
  absolute collection only moves the pointer.
