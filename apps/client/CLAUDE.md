# mixlink-client

> Module card. Keep <= 50 lines. Update when the public API changes.

**Purpose:** Client binary: device list (discovery), pairing PIN entry, stream window (winit), wires net->proto->codec->render, input capture.
**Depends on:** all crates.
**Hot path:** only via crates; this app wires threads and queues, no media logic here.

## Planned layout
- `main.rs`, `session.rs`, `threads.rs`, `ui/{device_list,settings}.rs`, `window.rs`

## Public API
- (none yet — fill in names of exported traits/types, one line each)

## Notes
- (gotchas an AI session must know before editing this module)
