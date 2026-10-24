# mixlink-proto

> Module card. Keep <= 50 lines. Update when the public API changes.

**Purpose:** Wire format: packet header, channel ids, (de)serialization, FEC encode/recover, AES-GCM framing. Pure logic, no sockets.
**Depends on:** mixlink-core; `thiserror`, `serde`, `serde_json`.
**Hot path:** yes — packetize/depacketize every packet; zero-copy, preallocated buffers.

## Layout
- `header.rs`, `channel.rs`, `packetizer.rs`, `reassembler.rs`, `control.rs`, `cursor.rs`, `error.rs`
  (each + `<name>_tests.rs`); `tests/no_alloc.rs` (counting allocator, test-only `unsafe`).
- Planned: `fec.rs` (T13), `crypto.rs` (T6).

## Public API
- `header`: `WIRE_VERSION`, `HEADER_LEN` (16), `MAX_PAYLOAD` (1200), `MAX_DATAGRAM`, `Flags`
  (`KEYFRAME`, `PARITY`), `PacketHeader` (`encode`, `decode` -> header + payload slice).
- `channel`: `Channel` (`Control`..`Feedback`, ids 1-6, `ALL`, `id`, `from_id`).
- `packetizer`: `fragment_count`, `Packetizer` (`new`, `with_seq`, `next_seq`, `packetize`),
  `Fragments` (`count`, `next_into`).
- `reassembler`: `ReassemblerConfig` (`for_channel`), `Reassembler` (`new`, `push`, `stats`),
  `Push`, `Message`, `DropReason`, `ReassemblerStats`.
- `control`: `ControlMessage` (`Hello`, `Welcome`, `Reject`, `Bye`; `validate`), `DisplayInfo`,
  `RejectReason`, `ByeReason`, `encode`, `decode`.
- `cursor`: `CursorPosition` (`encode`), `CursorShapeHeader` (`encode_into`), `ShapeKind`,
  `CursorMessage`, `decode`, `CURSOR_POSITION_LEN`, `CURSOR_SHAPE_HEADER_LEN`.
- `error`: `DecodeError`, `PacketizeError`, `ControlError`, `CursorError`.

## Notes
- Header layout is frozen by ADR-0008; any change bumps `WIRE_VERSION`.
- `Reassembler` allocates everything in `new`; `push` and `Fragments::next_into` must stay
  allocation-free (`tests/no_alloc.rs` checks it). One reassembler per channel and peer; not `Sync`.
- A single-packet message is returned borrowed from the datagram, a fragmented one from a slot;
  both borrows end at the next `push`.
- Late = not newer than the newest completed or evicted frame. Out-of-order completion gives up
  older pending frames (counted `evicted`); the caller decides on intra-refresh.
- Control messages are JSON (not hot path); cursor and header are fixed binary.
