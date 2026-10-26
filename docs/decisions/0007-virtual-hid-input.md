# 0007 — Host input through virtual HID devices

**Status:** accepted · 2026-10-22 · gamepad part superseded by
[ADR-0009](0009-virtual-xbox-pads.md) (virtual Xbox pads; no generic HID pad, no default ViGEmBus)

## Context
`SendInput` marks events as injected (`LLKHF_INJECTED` / `LLMHF_INJECTED`); some games and
anti-cheats ignore them, and UIPI blocks them toward higher-integrity windows. ViGEmBus is
retired and blacklisted by some anti-cheats. Kernel drivers are ruled out (ADR-0004).
A UMDF 2 HID minidriver (`vhidmini2` sample) exposes devices that Windows treats as hardware.

## Decision
- **Own virtual HID driver** (`drivers/vhid`, UMDF, ADR-0004) exposing keyboard, mouse
  (relative + absolute) and gamepad collections. The service feeds HID reports over IOCTL.
- **Keyboard / mouse:** virtual HID is the primary path. `SendInput` is the fallback when the
  driver is missing or failed to install.
- **Gamepad:** virtual HID gamepad for games that read HID / Raw Input / Windows.Gaming.Input.
- **XInput-only games:** XInput needs a kernel XUSB device we cannot ship. Optional component:
  ViGEmBus (author-signed), installed only on user consent, with its anti-cheat risk shown.
  Emulating a PlayStation pad (Sony VID/PID) for Steam Input is not done.
- Never hook, inject into or read memory of game processes. Never require test-signing.
- Wire format: input channel carries device-level events; the host maps them to HID reports.

## Consequences
- Input is not flagged as injected and is not blocked by UIPI; works on UAC and login screens.
- Risks to prove: spike S3 (UMDF HID keyboard/mouse collections are accepted and fast),
  spike S5 (anti-cheat launch: EA Javelin and EAC pass, spec 0008; gameplay and input untested, user
  warning in `docs/compat/anti-cheat.md`).
- Without ViGEmBus, XInput-only games get no gamepad. Shown in the per-game compatibility list.
- Replaces "SendInput + virtual gamepad" of ADR-0001; `mixlink-input` host side becomes a HID
  report builder plus `SendInput` fallback.
