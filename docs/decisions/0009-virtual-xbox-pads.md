# 0009 — Host gamepads are virtual Xbox pads on the inbox XInput filter

**Status:** accepted · 2026-10-26 · supersedes the gamepad part of ADR-0007

## Context
ADR-0007 planned a generic HID gamepad collection in our UMDF `vhid` driver, and ViGEmBus for
XInput-only games. Spec 0016 tested the generic pad and dropped it:
- Steam reads only the first axis INPUT item of a generic HID pad, and its setup wizard binds triggers
  unreliably (it assumes every axis rests at the centre). Every user would see "Begin Setup".
- XInput-only games (Wuchang) never see a generic HID pad.

Windows ships `xinputhid.inf` (Microsoft-signed). It binds to the HID hardware ID of the Xbox Series
controller over Bluetooth (`HID\VID_045E&PID_0B13&IG_00`) and adds the `xinputhid` upper filter, which
makes the device an XInput pad and a Windows.Gaming.Input `Gamepad`. No kernel driver of ours is needed.

## Decision
- **One virtual Xbox Series pad per client gamepad slot** (0..3): a node of our UMDF `vhid` driver whose
  HID descriptor and reports are the real Bluetooth controller's, VID `045E`, PID `0B13`.
- The node is a **software device** (`SWD\MixLinkPad\000N`, `SwDeviceCreate`) with hardware ID
  `Root\VID_045E&PID_0B13&IG_00`. hidclass derives the HID child's ID from it, so the inbox
  `xinputhid` binds. Each slot has its own container ID, or XInput skips a user index.
- The service creates a pad when a client gamepad appears and removes it (an unplug) when the gamepad
  leaves. Leftover pads go at service start and before `drivers uninstall`.
- ViGEmBus stays a fallback only if a game rejects these pads; it is not installed by default.
- Keyboard and mouse stay as in ADR-0007.

## Consequences
- Games, Steam and Windows see an Xbox controller with no setup step. XInput, WGI, DirectInput and
  Steam all read it; measured XInput p99 0.47 ms (spec 0016 Results).
- A virtual device carries Microsoft's VID/PID, as ViGEm, Sunshine and Steam do. The anti-cheat note
  (`docs/compat/anti-cheat.md`) says so.
- Pad nodes are per host, not per Windows session; several sessions sharing a host is not handled.
- Other personas (DualSense, Switch Pro) are later work, each needing its own inbox or Steam path.
- Rumble output (report 0x03) is accepted and kept per pad; sending it to the client is later work.
