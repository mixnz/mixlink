# mixlink-core

> Module card. Keep <= 50 lines. Update when the public API changes.

**Purpose:** Shared ids, config, error types, monotonic clock + per-frame stage timestamps.
**Depends on:** nothing internal (leaf crate); `windows` only for QPC (`cfg(windows)`).
**Hot path:** `clock::now` and `StageStamps::mark`/`mark_at` are called per frame: allocation-free.

## Layout
- `ids.rs`, `clock.rs`, `stage.rs`, `config.rs`, `ipc.rs`, `error.rs` (each + `<name>_tests.rs`)
- `platform/windows/qpc.rs` (QPC), `platform/fallback.rs` (`Instant`, non-Windows): the only platform code.

## Public API
- `ids`: `SessionId`, `FrameId`, `Seq` (`u32`; `new`, `get`; `FrameId`/`Seq`: `next`, `is_newer_than`
  with RFC 1982 order), `DeviceId` (`[u8; 16]`, 32-hex `Display`/`FromStr`/serde).
- `clock`: `Ticks` (`ZERO`, `from_raw`, `raw`, `duration_since`), `now()`, `ticks_per_sec()`.
- `stage`: `Stage` (8 stages, `COUNT`, `ALL`, `index`), `StageStamps` (`new`, `frame`, `mark`, `mark_at`,
  `get`, `span`).
- `config`: `CONFIG_VERSION`, `HostConfig`, `ClientConfig` (`from_json`, `to_json_pretty`, `validate`),
  `NetConfig`, `VideoConfig`.
- `error`: `IdParseError`, `ConfigError`.
- `ipc`: launcher <-> stream JSON-lines contract — `IPC_VERSION`, `StreamToLauncher`, `LauncherToStream`,
  `CloseReason`, `IpcError`, `encode_line`, `decode_line`.

## Notes
- `clock` ticks are raw QPC on Windows, same clock as `vdisplay` frame stamps; never subtract host
  and client ticks (different machines).
- `StageStamps` treats `Ticks::ZERO` as "not reached".
- Config: missing fields take defaults, unknown fields are ignored; name limit is 63 UTF-8 bytes.
- Stats counters come with T9 / T13.
