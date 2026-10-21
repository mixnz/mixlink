# mixlink-proto

> Module card. Keep <= 50 lines. Update when the public API changes.

**Purpose:** Wire format: packet header, channel ids, (de)serialization, FEC encode/recover, AES-GCM framing. Pure logic, no sockets.
**Depends on:** mixlink-core.
**Hot path:** yes — packetize/depacketize every packet; zero-copy, preallocated buffers.

## Planned layout
- `header.rs`, `channel.rs`, `packetizer.rs`, `reassembler.rs`, `fec.rs`, `crypto.rs`, `control.rs`

## Public API
- (none yet — fill in names of exported traits/types, one line each)

## Notes
- (gotchas an AI session must know before editing this module)
