# mixlink-input

> Module card. Keep <= 50 lines. Update when the public API changes.

**Purpose:** Client: capture keyboard, raw relative mouse, gamepad. Host: build HID reports for `drivers/vhid` (ADR-0007, spec 0016), SendInput fallback.
**Depends on:** mixlink-core (`device_io`).
**Hot path:** yes — every input event; send immediately, never batch per frame.

## Layout
- `platform/windows/vhid_wire.rs` (+ tests): IOCTL bytes = `drivers/vhid/Control.h`.
- `platform/windows/vhid.rs`: `VHid` feeder. FFI lives in `core::device_io`, so this crate has no `unsafe`.
- `hid/` (pure, no heap, tests in `hid_tests.rs`): `report.rs`, `keyboard.rs`, `mouse.rs`, `xbox.rs`;
  layouts = `drivers/vhid/Descriptor.c` (kbd / mouse) and `PadDescriptor.c` (Xbox pad).
- Planned: `event.rs`, `capture/`, `platform/windows/{raw_input,xinput,send_input}.rs`.

## Public API
- `hid::report`: `PAYLOAD_MAX` (16), `Target` (`Keyboard`, `MouseRel`, `MouseAbs`, `Pad(slot)`; `wire_id` 1 / 2 / 3 /
  0x10 + slot, `payload_len`), `Report` (`target`, `payload`), `neutral_reports()` (kbd, mouse rel, every pad slot).
- `hid::keyboard`: `KeyboardState` (`set(usage, down) -> bool`, `clear`, `report`; NKRO).
- `hid::mouse`: `REL_MAX`, `mouse_rel_reports(buttons, dx, dy, wheel, pan) -> RelReports` (≤ 4, split),
  `mouse_abs_report(buttons, x, y)`.
- `hid::xbox`: `PAD_SLOTS` (4), `buttons::{A, B, X, Y, LB, RB, VIEW, MENU, GUIDE, LS, RS}`, `Dpad`, `PadState`
  (`NEUTRAL`, `report(slot)`; sticks i16 +y = down, triggers u16 → 10-bit, `share`).
- `platform::windows::vhid_wire` (wire v2): `IOCTL_FEED`, `IOCTL_QUERY`, `WIRE_VERSION`, `QueryReply`
  (`pads_present`), `encode_feed`, `encode_query`, `decode_query`, `WireError`.
- `platform::windows::vhid`: `VHid::open` / `query` / `feed`, `ReportSink`, `VHidError` (`Missing`,
  `AccessDenied`, `InUse`, `NotPaired`, `Wire`, `Win32`).

## Notes
- The feed goes to the control node `Root\MixLinkVHidCtl` (SYSTEM + Admins only), which shares a WUDFHost
  with the HID nodes; an interface on a HID node is refused by hidclass (spec 0016).
- Pads are separate nodes `Root\VID_045E&PID_0B13&IG_00` per slot; the inbox `xinputhid` makes them XInput.
  A target whose pad has no node fails with `NotPaired`.
