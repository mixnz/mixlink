# mixlink-net

> Module card. Keep <= 50 lines. Update when the public API changes.

**Purpose:** UDP socket setup, send pacer, bitrate/congestion controller, LAN discovery (mDNS + broadcast), pairing handshake. Later: ICE/STUN/TURN.
**Depends on:** mixlink-proto, mixlink-core.
**Hot path:** send/recv loops and pacer; discovery and pairing are control plane (tokio ok).

## Planned layout
- `socket.rs`, `pacer.rs`, `bitrate.rs`, `discovery/{mdns,broadcast}.rs`, `pairing.rs`, `platform/windows/`

## Public API
- (none yet — fill in names of exported traits/types, one line each)

## Notes
- (gotchas an AI session must know before editing this module)
