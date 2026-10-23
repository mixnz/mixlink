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
- Spike S4 (spec 0007): loopback works from the Session 0 service, also before login.
- Shared-mode loopback packets come every 10 ms: `IAudioClient3` refuses the loopback flag
  (`AUDCLNT_E_INVALID_STREAM_FLAG`), so no low-latency period. Cut 10 ms into two 5 ms Opus frames.
- Loopback delivers **no packets while nothing plays** on the endpoint: fill silence on a timer.
- Follow default-device changes with `IMMNotificationClient` (flag only in the callback, reopen in the
  capture loop; ~12 ms). The first packet after opening carries the discontinuity flag.
