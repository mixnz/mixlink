# 0008 — Packet header v1

**Status:** accepted · 2026-10-24

## Context
Every datagram on the session socket (ADR-0002) carries one header. Later tasks build on it:
pacing (T4), encryption (T6), video / cursor (T9), input (T10), FEC (T13). The draft in
`architecture/protocol.md` had `fec_group` / `fec_index`, but reassembly needs the fragment count
on every packet, and the header must stay small. Spec 0010 implemented it in `crates/proto`.

## Decision
- **16 bytes, little-endian, plain** (authenticated as AES-GCM AAD from T6, not encrypted):

| Off | Field | Size | Meaning |
|---|---|---|---|
| 0 | version | 1 | `WIRE_VERSION = 1` |
| 1 | channel | 1 | 1 control, 2 video, 3 audio, 4 cursor, 5 input, 6 feedback; 0 invalid |
| 2 | flags | 2 | bit 0 keyframe, bit 1 FEC parity (T13); other bits invalid |
| 4 | seq | 4 | per channel and direction, +1 per datagram, wraps (RFC 1982 order) |
| 8 | frame_id | 4 | frame number (video / audio) or message number (other channels) |
| 12 | frag_index | 2 | fragment index, `< frag_count` |
| 14 | frag_count | 2 | fragments in the message, ≥ 1 |

- **Datagram budget:** 16 header + ≤ 1200 payload + 16 GCM tag (T6) = ≤ 1232 bytes of UDP payload,
  the IPv6 minimum MTU (1280) minus IPv6 and UDP headers: never IP-fragmented, LAN or internet.
- **No offset field:** every fragment but the last carries exactly 1200 bytes, so a fragment's
  offset is `frag_index * 1200`.
- **Strict decode:** unknown version, channel or flag bits, `frag_count = 0`,
  `frag_index ≥ frag_count`, payload > 1200, or a short non-last fragment drop the packet.
- **FEC (T13) keeps this header:** parity datagrams set the parity flag, use `frag_index` as their
  parity shard index, and start their payload with a small FEC sub-header naming the block.
- Any change to this layout bumps `WIRE_VERSION`.

## Consequences
- Up to 65535 fragments per message (about 78 MB); real limits are per channel in the reassembler.
- 1.3 % header overhead on full packets; 2.6 % with the GCM tag.
- A lost fragment loses the whole message until FEC (T13) lands.
- Code: `crates/proto/src/header.rs::PacketHeader`.
