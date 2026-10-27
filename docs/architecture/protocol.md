# Protocol (draft)

## Discovery (LAN)
Spec 0017; code `crates/net/src/discovery/`.
- mDNS / DNS-SD service `_mixlink._udp.local` through the Windows DNS-SD API (system responder, no socket
  of ours on 5353). TXT: `id` (host `DeviceId`, 32 hex), `name`, `ver` (`WIRE_VERSION`), `caps` (comma
  list, `v1`; unknown values ignored); SRV port = session port.
- Broadcast fallback on UDP `discovery_port` (47800): probe `MXLD` + `1` (5 B) to every interface's
  directed broadcast address every 1.5 s; reply = same 5 B + JSON `{id, name, ver, caps, port}`
  (≤ 1200 B), unicast, at most one per source IP per 250 ms. Anything else gets no reply.
- Client merges both by `id`; an entry no source has seen for 5 s is removed. mDNS goodbyes remove at
  once; the client re-resolves known instances every 1.5 s so live hosts stay fresh.
- Host id: `%ProgramData%\MixLink\identity.json`, random on first service start.

## Pairing (spec 0018; `crates/net/src/pairing/`)
- Plaintext control messages on the session port. `PairRequest {proto, device, host, name, key}` opens a
  prompt on the host (random 6-digit PIN, 120 s, one device at a time, else `Reject(Busy)`); reply `PairPrompt`.
- `PairStart {device, spake}` (SPAKE2 Ed25519, ids = client id, host id, password = PIN) → `PairReply {spake,
  key, mac}` → `PairConfirm {device, mac}` → `PairDone`. Transcript = SHA-256 of `mixlink-pair-v1`, both ids,
  both static keys, both SPAKE messages; MACs = HMAC-SHA256 with HKDF keys `confirm h` / `confirm c`.
- 3 `PairStart` attempts per prompt (a repeated message is not an attempt), then `Reject(PairingClosed)`.
  Wrong confirm → `Reject(BadPin)`. Keys: host `%ProgramData%\MixLink\secrets\` (SYSTEM + Admins), client
  `%LOCALAPPDATA%\MixLink\`.

## Session transport (spec 0018; `crates/net/src/session/`, `crates/proto/src/crypto.rs`)
- One UDP socket per side (default port 47801), all channels multiplexed.
- Handshake: Noise `KK_25519_AESGCM_SHA256`, prologue `mixlink-v1` ‖ client id ‖ host id.
  `SessionInit {proto, device, host, noise}` carries `Hello` in message 1; `SessionAccept {noise}` carries
  `Welcome` or `Reject` in message 2. Unknown device → plaintext `Reject(NotPaired)`.
- Keys: HKDF-SHA256 of the Noise split, infos `mixlink-v1 c2h` / `h2c`, first 16 bytes (AES-128).
- The host commits a session on the client's first sealed `Ping`; `Pong` answers; 10 s without a valid
  datagram ends it; sealed `Bye` ends it at once. A new session from the same device replaces the old one.
- Sealed datagram: header (AAD) ‖ AES-128-GCM payload ‖ 16 B tag. Nonce = `channel, 0, 0, 0` ‖ roc (u32 LE)
  ‖ seq (u32 LE); roc counts seq wraps. Receivers keep a 1024-datagram replay window per channel.
- Control: before a session plaintext only; after it sealed, plus plaintext `SessionInit` / pairing (a
  restarted client). Other channels are always sealed.

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

- Reassembly: a frame not newer than the newest completed or evicted one (or, with all slots busy,
  older than every pending one) is late and dropped; a newer frame completing first gives up older
  pending ones.

## Control handshake (`crates/proto/src/control.rs`)
- Client sends `Hello` (proto version, device id, name, 1–8 displays, `stream_display` index);
  host answers `Welcome` (session id, streamed mode) or `Reject` (version, busy, not paired, bad display).
- Display refresh is in millihertz (59.94 Hz = 59 940). Unknown JSON fields are ignored.
- Lost packets: the client resends `Hello` every 250 ms until a reply arrives, giving up after 10 s
  (`crates/net/src/resend.rs`, `handshake.rs`). A general ack +
  resend layer comes with the first non-idempotent control message. `Bye` ends a session.

## Cursor (`crates/proto/src/cursor.rs`)
- Byte 0 kind: 1 position (`x`, `y`, `visible`, `shape_id`, 14 B), 2 shape (18 B header + pixels).
- Shapes are sent on change; a position naming an unknown `shape_id` means the shape was lost (T9).
