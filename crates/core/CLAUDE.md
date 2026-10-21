# mixlink-core

> Module card. Keep <= 50 lines. Update when the public API changes.

**Purpose:** Shared ids, config, error types, monotonic clock + stage timestamps, stats counters.
**Depends on:** nothing internal (leaf crate).
**Hot path:** clock and stats are called per frame: must be allocation-free.

## Planned layout
- `config.rs`, `error.rs`, `clock.rs`, `stats.rs`, `ids.rs`

## Public API
- (none yet — fill in names of exported traits/types, one line each)

## Notes
- (gotchas an AI session must know before editing this module)
