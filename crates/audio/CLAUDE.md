# mixlink-audio

> Module card. Keep <= 50 lines. Update when the public API changes.

**Purpose:** WASAPI loopback capture, Opus encode/decode (5 ms frames), low-latency playback.
**Depends on:** mixlink-core.
**Hot path:** yes — audio thread runs every 5 ms; no allocation or locks.

## Planned layout
- `capture.rs`, `opus.rs`, `playback.rs`, `platform/windows/wasapi.rs`

## Public API
- (none yet — fill in names of exported traits/types, one line each)

## Notes
- (gotchas an AI session must know before editing this module)
