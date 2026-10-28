# mixlink-proto

> Module card. Keep <= 50 lines. Update when the public API changes.

**Purpose:** Wire format: packet header, channel ids, (de)serialization, FEC encode/recover, AES-GCM framing. Pure logic, no sockets.
**Depends on:** mixlink-core; `thiserror`, `serde`, `serde_json`, `aes-gcm`.
**Hot path:** yes — packetize/depacketize every packet; zero-copy, preallocated buffers.

## Layout
- `header.rs`, `channel.rs`, `packetizer.rs`, `reassembler.rs` (+ private `slot.rs`), `control.rs` (+ `blob.rs`), private `replay.rs`,
  `cursor.rs`, `video.rs`, `feedback.rs`, `input.rs`, `crypto.rs`, `error.rs` (each + `<name>_tests.rs`; `reassembler_stress_tests.rs` fuzz + timing;
  `test_util.rs` helpers);
  `tests/no_alloc.rs` (counting allocator, test-only `unsafe`), `tests/crypto_bench.rs` (ignored, by hand).
- Planned: `fec.rs` (T13).

## Public API
- `header`: `WIRE_VERSION`, `HEADER_LEN` (16), `MAX_PAYLOAD` (1200), `MAX_DATAGRAM`, `Flags`
  (`KEYFRAME`, `PARITY`), `PacketHeader` (`encode`, `decode` -> header + payload slice).
- `channel`: `Channel` (`Control`..`Feedback`, ids 1-6, `ALL`, `id`, `from_id`).
- `packetizer`: `fragment_count`, `Packetizer` (`new`, `with_seq`, `next_seq`, `packetize`),
  `Fragments` (`count`, `next_into`).
- `reassembler`: `ReassemblerConfig` (`for_channel`), `Reassembler` (`new`, `push`, `stats`),
  `Push`, `Message`, `DropReason`, `ReassemblerStats`.
- `control`: `ControlMessage` (`Hello`, `Welcome`, `Reject`, `Bye`, pairing `Pair*`, `SessionInit` / `Accept`, `Ping` / `Pong`;
  `validate`), `PublicKey` + `Blob` (hex in JSON, `blob::MAX_BLOB`), `SPAKE_LEN`, `MAC_LEN`, `MAX_NOISE`, `DisplayInfo`,
  `RejectReason`, `ByeReason` (`HostError`), `CodecId` (`Hello.codecs`, `Welcome.codec`), `encode`, `decode`.
- `video` (spec 0020): `FrameMeta` (`encode`, `split` -> meta + AU), `FRAME_META_LEN` (16).
- `feedback` (spec 0020): `Feedback` (`KeyframeRequest`, `CursorShapeRequest`; `encode`, `decode`), `FEEDBACK_LEN`.
- `input` (spec 0021): `InputPacket` (`encode`, `decode`), `InputEvent`, `MouseMode`, `INPUT_LEN` (80), `MAX_EVENTS` (8),
  `BUTTON_CODE` (0xF0), `KEY_BYTES`.
- `cursor`: `CursorPosition` (`encode`), `CursorShapeHeader` (`encode_into`), `ShapeKind`,
  `CursorMessage`, `decode`, `CURSOR_POSITION_LEN`, `CURSOR_SHAPE_HEADER_LEN`.
- `crypto` (spec 0018): `SessionKeys` (`c2h`, `h2c`; `sealer`, `opener`), `Role`, `Sealer` (`seal`), `Opener`
  (`open`, `stats`), `OpenStats`, `TAG_LEN`, `KEY_LEN`, `WINDOW` (1024).
- `error`: `DecodeError`, `PacketizeError`, `ControlError`, `CursorError`, `CryptoError`, `VideoError`, `FeedbackError`, `InputError`.

## Notes
- Header layout is frozen by ADR-0008; any change bumps `WIRE_VERSION`.
- `Reassembler` allocates everything in `new`; `push` and `Fragments::next_into` must stay
  allocation-free (`tests/no_alloc.rs` checks it). One reassembler per channel and peer; not `Sync`.
- A single-packet message is returned borrowed from the datagram, a fragmented one from a slot;
  both borrows end at the next `push`.
- Late = not newer than the newest completed or evicted frame, or (slots full) older than every
  pending frame. Out-of-order completion gives up older pending frames (counted `evicted`); the
  caller decides on intra-refresh.
- Control messages are JSON (not hot path); cursor and header are fixed binary.
