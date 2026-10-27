# mixlink-net

> Module card. Keep <= 50 lines. Update when the public API changes.

**Purpose:** UDP socket setup, send pacer, bitrate/congestion controller, LAN discovery (mDNS + broadcast), pairing handshake. Later: ICE/STUN/TURN.
**Depends on:** mixlink-proto, mixlink-core; `socket2`, `rtrb`, `log`, `thiserror`, `serde(_json)`, `snow`, `getrandom`; `windows`
(WinSock, IpHelper, DNS-SD).
**Hot path:** send/recv loops and lanes; discovery and pairing are control plane (plain threads).

## Layout
- `socket.rs` (open + configure), `lane.rs` (SPSC slot lanes), `transport.rs` (start / stop, lanes),
  `send_loop.rs`, `recv_loop.rs`, `stats.rs`, `resend.rs`, `handshake.rs`, `keys.rs`, `session/`, `error.rs`
  (each logic file + `<name>_tests.rs`); MMCSS via `core::mmcss`.
- `discovery/` (spec 0017): pure `advert.rs`, `wire.rs`, `list.rs`, `limiter.rs`, `local_net.rs`; threads
  `advertiser.rs` (`svc-discovery`), `browser.rs` (`discovery-browse`).
- `platform/windows/` (only `unsafe`): `socket.rs`, `adapters.rs` (IPv4 interfaces), `dns_sd.rs` (register),
  `dns_sd_browse.rs` (browse + resolve); `platform/fallback.rs` for other targets.
- `tests/no_alloc.rs`, `tests/latency.rs` (loopback p50 / p99), `tests/discovery_loopback.rs`;
  `examples/dnssd_probe.rs` (`register <name> [s]` / `browse`, manual mDNS checks).
- Planned: `pacer.rs`, `bitrate.rs` (phase 3), `pairing.rs`.

## Public API
- `transport`: `Transport` (`start`, `local_addr`, `stats`, `shutdown`), `TransportConfig` (`host`,
  `client`, `loopback`), `LaneCapacities` (`get`, `set`), `Lanes` (`take_tx`, `take_rx`).
- `lane`: `SLOT_LEN` (1232), `TxLane` (`try_reserve`), `TxSlot` (`buf`, `datagram`, `commit`, `seal_commit`),
  `RxLane` (`try_recv`, `recv_timeout`, `attach_current_thread`), `RxDatagram` (`data`, `peer`, `at`, `open`).
- `keys`: `StaticKeyPair` (`generate`; serde hex), `random_pin`, `random_session`.
- `stats`: `TransportStats` (`tx`, `rx` per channel, socket counters), `LaneCount`, `LaneCounters`.
- `socket`: `SocketConfig`, `OpenSocket`, `open`, `SOCKET_BUFFER` (4 MiB), `READ_TIMEOUT` (20 ms).
- `resend`: `ResendTimer` (`new`, `poll`), `ResendAction`, `HELLO_RESEND` (250 ms), `HELLO_GIVE_UP` (10 s).
- `handshake`: `connect`, `connect_with`, `HandshakeReply`.
- `error`: `NetError`.
- `discovery`: `Advert` (`txt`, `from_txt`, `caps_list`), `SERVICE_TYPE`, `CAPS_V1`; `Advertiser` (`start`,
  `local_port`, `stop`) + `AdvertiserConfig`; `Browser` (`start`, `snapshot`, `stop`) + `BrowserConfig`,
  `ProbeTargets`, `OnChange`, `DiscoveryEvent`; `HostList`, `HostEntry`, `Source`, `EXPIRY` (5 s);
  `LocalNet`; `wire` (`PROBE`, `encode_reply`, `decode_reply`); `DiscoveryError`.

## Notes
- Each lane has one owner thread on each end (`Send`, not `Clone`). An `RxDatagram` is valid until
  the next receive on its lane; an uncommitted `TxSlot` is reused by the next `try_reserve`.
- One thread serving several rx lanes: `attach_current_thread` on each, then `try_recv` +
  `std::thread::park_timeout`. The first attach per lane wins.
- Send priority: Control, Input, Feedback, Audio, Cursor, then Video in batches of 32.
- The recv thread routes on bytes 0-1 only; consumers run `PacketHeader::decode`. Every datagram
  carries its sender: filtering by session peer is the caller's job.
- `SIO_UDP_CONNRESET` is off: without it a closed peer port breaks the next `recv_from` on Windows.
- Slots have 16 B spare for the GCM tag (T6). `tx.passed` counts commits, not send successes.
- Discovery: `Advertiser::start` fails only when neither mDNS nor the responder starts. DNS-SD callback
  contexts that Windows may still use after a cancel are leaked on purpose (small, per browse).
