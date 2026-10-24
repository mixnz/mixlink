# 0010 — Protocol: packet header, channels, handshake, packetizer / reassembler

**Status:** done
**Date:** 2026-10-24
**Roadmap:** T3
**Plan:** [0010-protocol-packets](../plans/0010-protocol-packets.md) (local only, written by writing-plans skill)
**Crates touched:** `crates/proto` (+ ADR-0008, `docs/architecture/protocol.md`)

## Goal
One wire format that every later task (T4 UDP, T6 pairing, T9 video, T10 input, T11 audio, T13 FEC)
plugs into: a frozen 16-byte header, channel ids, a zero-allocation packetizer / reassembler that
splits any message into datagrams of ≤ 1200 B payload, the control handshake that carries the
client's displays (ADR-0006), and the cursor messages. Pure logic, fully unit-tested, no sockets.

## Scope
- In:
  - `header.rs`: `PacketHeader` encode / decode, `Flags`, `WIRE_VERSION`, size constants.
  - `channel.rs`: `Channel` enum (`#[repr(u8)]`).
  - `packetizer.rs`: message → datagrams, per-channel `Seq`, pulled one datagram at a time.
  - `reassembler.rs`: datagrams → messages, preallocated slots, loss / late / duplicate counters.
  - `control.rs`: control messages incl. handshake (`Hello` with displays, `Welcome`, `Reject`, `Bye`).
  - `cursor.rs`: cursor position and shape messages (fixed binary layout).
  - `error.rs`: typed `DecodeError` / `PacketizeError` / `ControlError`.
  - ADR-0008 "Packet header v1" freezes the header; `protocol.md` updated to match.
- Out:
  - Sockets, threads, timers, pacing (T4). Control reliability (resend / ack) → T4, see Design.
  - Encryption (T6): only the size budget and "header = AAD" rule are fixed here.
  - FEC (T13): only the reserved `PARITY` flag and the layout rule below are fixed here.
  - Input (T10) and feedback (T13) message bodies: channel ids exist, bodies come with those tasks.
  - Video / audio frame content: opaque bytes to proto.

## Design

### Datagram budget
| Part | Bytes | Note |
|---|---|---|
| header | 16 | plain, authenticated as AAD from T6 |
| payload | ≤ 1200 | `MAX_PAYLOAD` |
| AES-GCM tag | 16 | added in T6 |
| **UDP payload** | **≤ 1232** | = IPv6 minimum MTU 1280 − 40 (IPv6) − 8 (UDP): never fragments at IP level, LAN or internet |

### Header v1 (`header.rs`, little-endian, 16 B)
| Off | Field | Size | Meaning |
|---|---|---|---|
| 0 | `version` | 1 | `WIRE_VERSION = 1`; other values → `DecodeError::Version` |
| 1 | `channel` | 1 | `Channel` id; unknown → `DecodeError::Channel` |
| 2 | `flags` | 2 | `KEYFRAME` (bit 0), `PARITY` (bit 1, reserved for T13); unknown bits → error |
| 4 | `seq` | 4 | `Seq`, per channel and direction, +1 per datagram (loss detection, T6 nonce) |
| 8 | `frame_id` | 4 | `FrameId`: frame number (video / audio) or message number (other channels) |
| 12 | `frag_index` | 2 | fragment index inside the message, `< frag_count` |
| 14 | `frag_count` | 2 | fragments in the message, ≥ 1 |

- Change vs the `protocol.md` draft: `fec_group` / `fec_index` become `frag_index` / `frag_count`.
  Reassembly needs the count on every packet; FEC (T13) keeps the 16 B header: parity datagrams
  carry `PARITY`, their `frag_index` is the parity shard index, and a small FEC sub-header at the
  start of their payload names the block (data shards are mapped by `frag_index`). T13 confirms.
- `PacketHeader` is a plain `Copy` struct; `encode(&self, &mut [u8; 16])`,
  `decode(&[u8]) -> Result<(PacketHeader, &[u8]), DecodeError>` (`// HOT PATH`, no allocation).
- Payload rules checked by `decode`: payload ≤ `MAX_PAYLOAD`; every fragment except the last has
  exactly `MAX_PAYLOAD` bytes (so offset = `frag_index * MAX_PAYLOAD`, no offset field needed).
- Message size limit: `frag_count ≤ u16::MAX` → about 78 MB; real limits are per channel (below).

### Channels (`channel.rs`)
| Id | Channel | Dir | Fragmented | Reassembler default (slots, max message) |
|---|---|---|---|---|
| 1 | `Control` | both | yes | 2, 64 KiB |
| 2 | `Video` | host→client | yes | 4, 4 MiB |
| 3 | `Audio` | host→client | no (1 packet) | 2, 1200 B |
| 4 | `Cursor` | host→client | shape only | 2, 264 KiB (256×256 BGRA + shape header) |
| 5 | `Input` | client→host | no | 2, 1200 B |
| 6 | `Feedback` | client→host | no | 2, 1200 B |

Id 0 is invalid (a zeroed buffer never parses as a packet).

### Packetizer (`packetizer.rs`)
- `Packetizer::new(channel)` holds the next `Seq`. `packetize(frame, flags, &message)` returns a
  `Fragments` cursor; `Fragments::next_into(&mut [u8; MAX_DATAGRAM]) -> Option<usize>` writes header
  + payload chunk and returns the datagram length. One copy (message → datagram buffer), which T6
  needs anyway as the encryption output buffer.
- Pull model on purpose: the T4 pacer takes one datagram at a time and spreads a frame over time.
- Empty message → 1 datagram with empty payload. Too many fragments → `PacketizeError::TooLarge`.
- `// HOT PATH`: no allocation, no lock, no logging.

### Reassembler (`reassembler.rs`)
- One per channel and peer, owned by one thread (not `Sync`). `Reassembler::new(ReassemblerConfig
  { slots, max_message_len })` allocates everything up front: `slots` buffers of `max_message_len`
  plus a received-fragment bitmap each. Nothing allocates after `new`.
- `push(header, payload) -> Push<'_>`:
  - `frag_count == 1`: returns `Push::Complete` borrowing the datagram payload directly (no copy).
  - Otherwise copies the payload to `frag_index * MAX_PAYLOAD` in the frame's slot; when the last
    missing fragment arrives returns `Push::Complete { frame, flags, data: &[u8] }` borrowing the slot.
  - Else `Push::Pending`, or `Push::Dropped(reason)`.
- Rules: a frame not newer than the newest completed or evicted one is `Late`; a fragment seen twice is
  `Duplicate`; `frag_count` or flags differing from the slot's first fragment, or a message over
  `max_message_len`, is `Malformed`. A new frame with all slots busy evicts the oldest slot
  (counted `Evicted`, i.e. that frame is lost), or is `Late` if it is older than every pending
  frame. Out-of-order completion is delivered; older pending frames then become late. The caller (T9 / T13) decides on intra-refresh.
- `stats() -> ReassemblerStats` (plain counters: completed, evicted, late, duplicate, malformed,
  seq gaps). Wrap-safe ordering via `FrameId::is_newer_than` / `Seq::is_newer_than`.

### Control messages and handshake (`control.rs`)
- Not a hot path: `ControlMessage` is a serde enum encoded as JSON (`serde`, `serde_json` already in
  the workspace, used by `core::ipc`): no new dependency, readable in logs and captures, unknown
  fields ignored so later tasks add fields without a version bump. Big messages fragment like any other.
- `encode(&ControlMessage) -> Result<Vec<u8>, ControlError>`, `decode(&[u8]) -> Result<ControlMessage, ControlError>`.
- Messages in T3:
  - `Hello { proto: u8, device: DeviceId, name, displays: Vec<DisplayInfo>, stream_display: u8 }`
    (client → host). `DisplayInfo { width, height, refresh_mhz: u32, hdr: bool, dpi: u16 }`;
    `refresh_mhz` in millihertz so 59.94 Hz is exact. `stream_display` = index of the display the
    stream window is on; the host builds its virtual monitor from it (ADR-0006).
  - `Welcome { session: SessionId, mode: DisplayInfo }` (host → client): the mode actually streamed.
  - `Reject { reason: RejectReason }`: `Version`, `Busy`, `NotPaired`, `BadDisplay`.
  - `Bye { reason: ByeReason }`: `UserClosed`, `Shutdown`, `Timeout`.
- Validation in `decode`: 1–8 displays, `stream_display` in range, width / height 1–16384,
  refresh 1–1000 Hz, name 1–63 bytes (same limit as core config).
- Reliability: the handshake is idempotent request → reply; the client resends `Hello` until it gets
  `Welcome` / `Reject` (rule written in `protocol.md`, timer lives in T4). A general ack + resend
  layer is added when the first non-idempotent control message needs it (T6 at the earliest).
- Pairing / auth (T6) wraps or precedes this; the messages here do not carry keys.

### Cursor messages (`cursor.rs`, fixed binary, little-endian)
- First payload byte = kind. `Position` (1 packet, `// HOT PATH`): `x: i32, y: i32` (virtual-monitor
  pixels, hotspot position), `visible: bool`, `shape_id: u32`.
- `Shape` (fragmented): `shape_id: u32, kind: ShapeKind (Color | MaskedColor | Monochrome: IddCx alpha +
  masked, DDA-fallback monochrome), width: u16, height: u16, hot_x: u16, hot_y: u16, pitch: u32`,
  then `pitch × height` pixel bytes.
  Encoding writes into a caller buffer; decoding borrows the pixel slice (no copy).
- Latest-wins: shapes are sent on change; a position naming an unknown `shape_id` means the shape
  was lost. Recovery (resend / request) is decided in T9.

### Threads
None created. `Packetizer` lives on the sender (T4 pacer) thread, `Reassembler` on the receiver
thread; both are `Send`, neither `Sync`.

## Latency impact
Adds header encode / decode (a few ns) and one memcpy per fragment (1200 B, tens of ns) on the
send and receive paths; single-packet messages are not copied on receive. Measured by a unit test
that packetizes and reassembles a 1 MiB frame 1000 times and prints µs per frame (informational,
`--nocapture`), as in spec 0009. No new dependency (no criterion).
Measured: 1 MiB frame packetize + reassemble 41 µs (release), 329 µs (debug).

## Acceptance criteria
- [x] Header round-trips for every channel / flag; each bad field (version, channel 0 / unknown,
      unknown flag bit, `frag_index ≥ frag_count`, `frag_count 0`, short buffer, payload > 1200,
      non-last fragment ≠ 1200 B) is rejected with its own `DecodeError`.
- [x] Packetize → shuffle → reassemble returns the original bytes for sizes 0, 1, 1200, 1201 and
      1 MiB; `Seq` increases by 1 per datagram and wraps; `FrameId` ordering holds across `u32::MAX`.
- [x] Lost fragment → frame never completes and is counted evicted when slots run out; duplicates,
      late frames and inconsistent `frag_count` are dropped and counted; nothing panics on 100k random
      datagrams (deterministic PRNG, no new dependency).
- [x] `Reassembler::push` and `Fragments::next_into` do not allocate (checked by a counting global
      allocator in an integration-test binary; its `unsafe impl GlobalAlloc` is test-only and
      delegates to `System`).
- [x] `Hello` / `Welcome` / `Reject` / `Bye` round-trip through JSON; each validation rule rejects its
      bad value; an unknown extra field is ignored.
- [x] Cursor `Position` and `Shape` round-trip; truncated buffers are errors, not panics.
- [x] ADR-0008 freezes header v1; `protocol.md` matches the code; `crates/proto/CLAUDE.md` lists the
      public API.

## Open questions
- None blocking. Calls made in this spec (review them): `frag_index` / `frag_count` replace the FEC
  fields; JSON for control; control reliability deferred to T4 / T6; strict rejection of unknown
  flag bits; per-channel defaults in the Channels table.
