# 0002 — Custom UDP transport instead of WebRTC/QUIC

**Status:** proposed · 2026-10-21

## Context
WebRTC brings jitter buffers, congestion control and pacing tuned for video calls
(adds 20–100 ms). QUIC streams are reliable/ordered: head-of-line blocking on loss.

## Decision
Own UDP protocol: FEC instead of retransmit for media, redundant input packets,
a tiny reliable sub-layer for control, own bitrate controller driven by client feedback.
For the internet phase reuse only ICE pieces (STUN/TURN, hole punching), not a media stack.

## Consequences
- More code to own (packetizer, FEC, pacing, congestion control) -> must be heavily
  unit-tested and tested under `tools/packet-sim`.
- Full control over every millisecond on the path.
