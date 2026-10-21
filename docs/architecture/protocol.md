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

## Packet header (little-endian, fixed 16 bytes before payload)
| Field | Size | Note |
|---|---|---|
| version | 1 | protocol version |
| channel | 1 | video / audio / input / control / feedback |
| flags | 2 | FEC parity, frame end, keyframe, ... |
| seq | 4 | per-channel sequence |
| frame_id | 4 | video/audio frame number |
| fec_group | 2 | FEC block index |
| fec_index | 2 | shard index inside block |

Payload <= 1200 bytes (safe MTU for the internet phase). Header is frozen by an ADR once implemented.
