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
- Spike S4 (spec 0007): FFmpeg 8.x via `ffmpeg-sys-next` needs libclang to build. In the Session 0
  service, encode only matches a user-session process with raised priority (ADR-0005 → Encode priority).
- NVENC takes `AV_PIX_FMT_D3D11` frames directly (BGRA or NV12). NV12 texture arrays with the
  render-target bind flag fail (`E_INVALIDARG`): use single textures or a decoder-bind pool + GPU copy.
- QSV: derive the QSV device from a D3D11 device with multithread protection on (else MFX error -16);
  pool with `D3D11_BIND_DECODER`; `hevc_qsv` has no `look_ahead` option.
