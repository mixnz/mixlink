# mixlink-input

> Module card. Keep <= 50 lines. Update when the public API changes.

**Purpose:** Client: capture keyboard, raw relative mouse, gamepad. Host: build HID reports for `drivers/vhid` (ADR-0007, spec 0016), SendInput fallback.
**Depends on:** mixlink-core (`device_io`).
**Hot path:** yes — every input event; send immediately, never batch per frame.

## Layout
- `platform/windows/vhid_wire.rs` (+ tests): IOCTL bytes = `drivers/vhid/Control.h`.
- `platform/windows/vhid.rs`: `VHid` feeder. FFI lives in `core::device_io`, so this crate has no `unsafe`.
- Planned: `event.rs`, `capture/`, `hid/` (report builders), `platform/windows/{raw_input,xinput,send_input}.rs`.

## Public API
- `platform::windows::vhid_wire`: `IOCTL_FEED`, `IOCTL_QUERY`, `WIRE_VERSION`, `QueryReply`, `encode_query`,
  `decode_query`, `WireError`.
- `platform::windows::vhid`: `VHid::open` / `query`, `VHidError` (`Missing`, `AccessDenied`, `InUse`,
  `NotPaired`, `Wire`, `Win32`).

## Notes
- The feed goes to the control node `Root\MixLinkVHidCtl` (SYSTEM + Admins only), which shares a WUDFHost
  with the HID node `Root\MixLinkVHid`; an interface on the HID node is refused by hidclass (spec 0016).
