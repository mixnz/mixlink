# mixlink-input

> Module card. Keep <= 50 lines. Update when the public API changes.

**Purpose:** Client: capture keyboard, raw relative mouse, gamepad. Host: inject via SendInput and virtual gamepad.
**Depends on:** mixlink-core.
**Hot path:** yes — every input event; send immediately, never batch per frame.

## Planned layout
- `event.rs` (shared event types), `capture/`, `inject/`, `platform/windows/{raw_input,xinput,send_input,vpad}.rs`

## Public API
- (none yet — fill in names of exported traits/types, one line each)

## Notes
- (gotchas an AI session must know before editing this module)
