# mixlink-host

> Module card. Keep <= 50 lines. Update when the public API changes.

**Purpose:** Host Tauri app (tray + React UI in `ui/`, ADR-0001), discovery announce, pairing, session wiring capture->codec->proto->net, input inject.
**Depends on:** all crates.
**Hot path:** only via crates; this app wires threads and queues, no media logic here.

## Planned layout
- `src/main.rs` (Tauri + tray), `src/commands.rs`, `src/session.rs`, `src/threads.rs`, `ui/` (React + Vite)

## Public API
- (none yet — fill in names of exported traits/types, one line each)

## Notes
- (gotchas an AI session must know before editing this module)
