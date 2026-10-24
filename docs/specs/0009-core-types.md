# 0009 — Core types

**Status:** in-progress
**Date:** 2026-10-24
**Roadmap:** T2
**Plan:** [0009-core-types](../plans/0009-core-types.md) (local only, written by writing-plans skill)
**Crates touched:** `crates/core`

## Goal
Every later crate (proto, net, svc, capture, codec, stream) shares one set of ids, one config
shape, one monotonic clock and one per-frame stage record, so T3+ never invent their own and
`tools/latency-probe` can join timestamps from the driver, the service and the client.

## Scope
- In:
  - `ids.rs`: newtypes `SessionId`, `FrameId`, `Seq`, `DeviceId`.
  - `clock.rs`: monotonic tick clock + conversions.
  - `stage.rs`: `Stage` enum + `StageStamps` (fixed array, no allocation).
  - `config.rs`: `HostConfig`, `ClientConfig` (+ nested sections), defaults, validation, JSON load/save from text.
  - `error.rs`: `ConfigError`, `IdParseError` (no crate-wide catch-all error).
- Out:
  - File paths / persistence location (`%ProgramData%`, `%APPDATA%`): apps resolve them (T19, client).
  - Clock offset between host and client machines (T9 latency report).
  - `stats.rs` counters (T9 / T13), id generation from keys (T6), channel ids (T3, proto).
  - UI language setting (roadmap row not added yet).

## Design

### Ids (`ids.rs`)
| Type | Repr | Note |
|---|---|---|
| `SessionId` | `u32` | One streaming session; nonce part in T6. Random value given by caller. |
| `FrameId` | `u32` | Video/audio frame number (header `frame_id`), wraps. |
| `Seq` | `u32` | Per-channel packet sequence, wraps; `is_newer_than` uses RFC 1982 serial arithmetic. |
| `DeviceId` | `[u8; 16]` | Stable machine id (mDNS TXT `id`, EDID per client). `Display` / `FromStr` as 32 lowercase hex. |

- All `Copy`, `Eq`, `Hash`, `Debug`; `FrameId` / `Seq` have `next()` (wrapping). Serde as number / hex string.

### Clock (`clock.rs`)
- `Ticks(u64)`: raw monotonic ticks. `now() -> Ticks` (`// HOT PATH`, no allocation, no lock).
- `ticks_per_sec()` cached once; `Ticks::duration_since(earlier) -> Duration`, `as_micros()` for reports.
- Windows: `QueryPerformanceCounter` in `platform/windows/` — the same clock the `vdisplay` driver
  writes into the frame header (spike S2), so driver and service stamps compare directly.
- Other targets: `std::time::Instant` against a process-wide anchor (tests / tooling only).

### Stage stamps (`stage.rs`)
- `Stage`: `Captured`, `EncodeStart`, `EncodeEnd`, `Sent` (host); `Received`, `DecodeStart`,
  `DecodeEnd`, `Presented` (client); `#[repr(u8)]`, `Stage::ALL`.
- `StageStamps { frame: FrameId, stamps: [Ticks; Stage::COUNT] }` (0 = not reached):
  `mark(stage)`, `mark_at(stage, ticks)`, `get(stage) -> Option<Ticks>`, `span(from, to) -> Option<Duration>`.
- Plain `Copy` struct so it can ride SPSC queues next to each frame. Host and client stamps are never
  subtracted across machines here.

### Config (`config.rs`)
- `HostConfig { version, name, net: NetConfig, video: VideoConfig }`,
  `ClientConfig { version, name, net: NetConfig }`.
- `NetConfig { discovery_port: 47800, session_port: 47801 }` (protocol.md defaults).
- `VideoConfig { max_fps: 120, bitrate_kbps: 50_000 }` (codec choice comes with T7).
- `#[serde(default)]` on every struct: a file missing fields still loads; unknown fields ignored.
- `from_json(&str) -> Result<_, ConfigError>` runs `validate()`; `to_json_pretty()`.
  `validate`: ports ≠ 0 and different, fps 1–240, bitrate 1_000–500_000, name 1–63 bytes.
- `version` ≠ `CONFIG_VERSION` → `ConfigError::UnsupportedVersion` (no migration until one is needed).

## Latency impact
Adds `now()` and `StageStamps::mark` to every stage later: one QPC read (~20–30 ns) and an array
store each, no allocation. Measured by a unit test timing 1M `now()` calls (informational, printed with
`--nocapture`), not a criterion bench (no new dependency).

## Acceptance criteria
- [ ] `Seq` / `FrameId` ordering is correct across wrap (`u32::MAX` → 0) in unit tests.
- [ ] `DeviceId` round-trips through `Display` / `FromStr` and serde; bad hex / length is an error.
- [ ] `now()` never goes backwards over 1M calls; `duration_since` matches `ticks_per_sec`.
- [ ] `StageStamps`: unmarked stage → `None`; `span` of two marked stages is their difference.
- [ ] Default `HostConfig` / `ClientConfig` round-trip through JSON; `{}` loads as defaults;
      each validation rule rejects its bad value with a typed `ConfigError`.
- [ ] `crates/core/CLAUDE.md` lists the new public API; no new dependency in `mixlink-core`
      besides `windows` (QPC, `cfg(windows)` only).

## Open questions
- None. Decided at review: core may hold one `platform/windows/qpc.rs` (root CLAUDE.md module map
  updated); `SessionId` is `u32` (nonce layout final in T6).
