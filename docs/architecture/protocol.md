# Protocol (draft)

## Discovery (LAN)
- mDNS / DNS-SD service `_mixlink._udp.local`, TXT: `id`, `name`, `ver`, `caps`.
- Fallback: UDP broadcast probe on port 47800, host replies with the same fields.
- Client lists hosts; stale entries expire after 5 s without announce.

## Pairing
- First connection: host shows a 6-digit PIN, client enters it.
- Key agreement: X25519 + PIN-authenticated confirm (SPAKE2 or HMAC), store peer public key.
- Later sessions: mutual auth with stored keys, no PIN.

## Session transport
- One UDP socket per side (default port 47801), all channels multiplexed.
- Payload encryption: AES-128-GCM (AES-NI), nonce = session id + packet seq.

## Packet header v1 (frozen: [ADR-0008](../decisions/0008-packet-header-v1.md))
- 16 bytes little-endian: `version`, `channel`, `flags`, `seq`, `frame_id`, `frag_index`, `frag_count`.
- Payload ≤ 1200 B; with the GCM tag the UDP payload is ≤ 1232 B (no IP fragmentation).
- Code: `crates/proto/src/header.rs`; split / rebuild messages: `packetizer.rs`, `reassembler.rs`.

## Channels
| Id | Channel | Dir | Messages | Reassembler (slots, max message) |
|---|---|---|---|---|
| 1 | control | both | JSON, fragmented | 2, 64 KiB |
| 2 | video | host→client | encoded frames, fragmented | 4, 4 MiB |
| 3 | audio | host→client | one packet per Opus frame | 2, 1200 B |
| 4 | cursor | host→client | position (1 packet), shape (fragmented) | 2, 264 KiB |
| 5 | input | client→host | one packet (body: T10) | 2, 1200 B |
| 6 | feedback | client→host | one packet (body: T13) | 2, 1200 B |

- Reassembly: a frame not newer than the newest completed or evicted one is late and dropped; a
  newer frame completing first gives up older pending ones.

## Control handshake (`crates/proto/src/control.rs`)
- Client sends `Hello` (proto version, device id, name, 1–8 displays, `stream_display` index);
  host answers `Welcome` (session id, streamed mode) or `Reject` (version, busy, not paired, bad display).
- Display refresh is in millihertz (59.94 Hz = 59 940). Unknown JSON fields are ignored.
- Lost packets: the client resends `Hello` until a reply arrives (timer in T4). A general ack +
  resend layer comes with the first non-idempotent control message. `Bye` ends a session.

## Cursor (`crates/proto/src/cursor.rs`)
- Byte 0 kind: 1 position (`x`, `y`, `visible`, `shape_id`, 14 B), 2 shape (18 B header + pixels).
- Shapes are sent on change; a position naming an unknown `shape_id` means the shape was lost (T9).
