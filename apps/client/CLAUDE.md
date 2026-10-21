# mixlink-client

> Module card. Keep <= 50 lines. Update when the public API changes.

**Purpose:** Client Tauri launcher (React UI in `ui/`, ADR-0001): device list (discovery), pairing PIN entry, settings; spawns `mixlink-stream` on Connect.
**Depends on:** all crates.
**Hot path:** only via crates; this app wires threads and queues, no media logic here.

## Planned layout
- `src/main.rs` (Tauri setup), `src/commands.rs` (Tauri commands), `src/stream_proc.rs` (spawn + IPC), `ui/` (React + Vite)

## Public API
- (none yet — fill in names of exported traits/types, one line each)

## Notes
- (gotchas an AI session must know before editing this module)
