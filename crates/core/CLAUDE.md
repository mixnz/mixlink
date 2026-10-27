# mixlink-core

> Module card. Keep <= 50 lines. Update when the public API changes.

**Purpose:** Shared ids, config, error types, monotonic clock + per-frame stage timestamps.
**Depends on:** nothing internal (leaf crate); `windows` only for QPC, MMCSS and `device_io` (`cfg(windows)`), `log`.
**Hot path:** `clock::now` and `StageStamps::mark`/`mark_at` are called per frame: allocation-free.

## Layout
- `ids.rs`, `hex.rs`, `clock.rs`, `stage.rs`, `config.rs`, `ipc.rs`, `svc_ipc.rs`, `error.rs`, `mmcss.rs` (each + `<name>_tests.rs`)
- `platform/windows/{qpc,mmcss,device_io}.rs`, `platform/fallback.rs` (`Instant`, no MMCSS; non-Windows): the only platform code.

## Public API
- `ids`: `SessionId`, `FrameId`, `Seq` (`u32`; `new`, `get`; `FrameId`/`Seq`: `next`, `is_newer_than`
  with RFC 1982 order), `DeviceId` (`[u8; 16]`, 32-hex `Display`/`FromStr`/serde).
- `clock`: `Ticks` (`ZERO`, `from_raw`, `raw`, `duration_since`), `now()`, `ticks_per_sec()`.
- `stage`: `Stage` (8 stages, `COUNT`, `ALL`, `index`), `StageStamps` (`new`, `frame`, `mark`, `mark_at`,
  `get`, `span`).
- `config`: `CONFIG_VERSION`, `MAX_NAME_BYTES` (63, shared with `proto::control`), `HostConfig`,
  `ClientConfig` (`from_json`, `to_json_pretty`, `validate`), `NetConfig`, `VideoConfig`.
- `hex`: `encode` (lowercase), `decode_into`, `decode_vec` (both cases accepted), `digit`.
- `error`: `IdParseError`, `HexError`, `ConfigError`.
- `mmcss`: `MmcssTask` (`ProAudio` critical, `Games` high), `Mmcss::enter(task) -> Option<Mmcss>` (reverts on drop,
  same thread; `None` + warning when refused).
- `ipc`: launcher <-> stream JSON-lines contract — `IPC_VERSION`, `StreamToLauncher`, `LauncherToStream`,
  `CloseReason`, `IpcError`, `encode_line`, `decode_line`.
- `svc_ipc`: tray <-> service pipe contract (spec 0012, 0018) — `PIPE_VERSION` (2), `PIPE_NAME`, `SERVICE_NAME`, `MAX_LINE`, `TrayToSvc`,
  `SvcToTray` (`Status` has `network_public`, `pairing`; `Paired`, `Done`), `PairingPrompt`, `PairedInfo`,
  `SvcState`, `StatusReport` (`from_status`), `read_line_limited` / `LineRead`, blocking client `SvcPipe` (`connect`,
  `request`, `status`, `cancel_pairing`, `list_paired`, `remove_paired`, `file`), `open_pipe` (read + write-data access only, see spec 0012), `SvcPipeError` (`NotRunning` =
  pipe missing or busy).
- `device_io` (Windows): `open_interface(&GUID) -> Result<Option<OwnedHandle>>` (first present interface,
  read + write; `None` = absent), `ioctl(&handle, code, input, output) -> Result<usize>`. Used for `vdisplay`, `vhid`.

## Notes
- `clock` ticks are raw QPC on Windows, same clock as `vdisplay` frame stamps; never subtract host
  and client ticks (different machines).
- `StageStamps` treats `Ticks::ZERO` as "not reached".
- Config: missing fields take defaults, unknown fields are ignored; name limit is 63 UTF-8 bytes.
- Stats counters come with T9 / T13.
