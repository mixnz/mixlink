# mixlink-codec

> Module card. Keep <= 50 lines. Update when the public API changes.

**Purpose:** Hardware video encode (host) and decode (client): H.264/HEVC/AV1, low-latency settings, intra-refresh.
**Depends on:** mixlink-core.
**Hot path:** yes — encode/decode per frame; GPU textures in, bitstream slices out (and reverse).

## Planned layout
- `encoder.rs` (trait), `decoder.rs` (trait), `settings.rs`, `backend/{ffmpeg,nvenc,amf}.rs`

## Public API
- (none yet — fill in names of exported traits/types, one line each)

## Notes
- (gotchas an AI session must know before editing this module)
