# mixlink-host

> Module card. Keep <= 50 lines. Update when the public API changes.

**Purpose:** Host binary: tray UI (egui), discovery announce, pairing, session wiring capture->codec->proto->net, input inject.
**Depends on:** all crates.
**Hot path:** only via crates; this app wires threads and queues, no media logic here.

## Planned layout
- `main.rs`, `session.rs`, `threads.rs`, `ui/`

## Public API
- (none yet — fill in names of exported traits/types, one line each)

## Notes
- (gotchas an AI session must know before editing this module)
