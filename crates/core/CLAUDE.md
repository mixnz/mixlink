# mixlink-core

> Module card. Keep <= 50 lines. Update when the public API changes.

**Purpose:** Shared ids, config, error types, monotonic clock + stage timestamps, stats counters.
**Depends on:** nothing internal (leaf crate).
**Hot path:** clock and stats are called per frame: must be allocation-free.

## Planned layout
- `ipc.rs` (+ `ipc_tests.rs`), `config.rs`, `error.rs`, `clock.rs`, `stats.rs`, `ids.rs`

## Public API
- `ipc`: launcher <-> stream JSON-lines contract — `IPC_VERSION`, `StreamToLauncher`, `LauncherToStream`,
  `CloseReason`, `IpcError`, `encode_line`, `decode_line`.

## Notes
- (gotchas an AI session must know before editing this module)
