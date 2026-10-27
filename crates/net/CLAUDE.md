# mixlink-net

> Module card. Keep <= 50 lines. Update when the public API changes.

**Purpose:** UDP socket setup, send pacer, bitrate/congestion controller, LAN discovery (mDNS + broadcast), pairing + secure session handshake. Later: ICE/STUN/TURN.
**Depends on:** mixlink-proto, mixlink-core; `socket2`, `rtrb`, `log`, `thiserror`, `serde(_json)`, `snow`, `spake2`,
`hkdf`, `hmac`, `sha2`, `getrandom`; `windows` (WinSock, IpHelper, DNS-SD).
**Hot path:** send/recv loops and lanes; discovery and pairing are control plane (plain threads).

## Layout
- `socket.rs`, `lane.rs` (SPSC slot lanes), `transport.rs`, `send_loop.rs`, `recv_loop.rs`, `stats.rs`, `resend.rs`,
  `handshake.rs`, `keys.rs`, `control_link.rs`, `endpoint.rs` (+ `endpoint_peer.rs`), `mode_policy.rs`, `client_store.rs`,
  `error.rs` (logic files + `<name>_tests.rs`).
- `pairing/` (spec 0018): `transcript.rs`, pure `host.rs`, `client.rs`; `session/`: `noise.rs`, pure `host.rs`, `client.rs`.
- `discovery/` (spec 0017): pure `advert.rs`, `wire.rs`, `list.rs`, `limiter.rs`, `local_net.rs`; threads
  `advertiser.rs` (`svc-discovery`), `browser.rs` (`discovery-browse`).
- `platform/windows/` (only `unsafe`): `socket.rs`, `adapters.rs`, `dns_sd.rs`, `dns_sd_browse.rs`; `platform/fallback.rs`.
- `tests/`: `no_alloc.rs`, `latency.rs`, `discovery_loopback.rs`, `pair_loopback.rs`; `examples/dnssd_probe.rs`.
- Planned: `pacer.rs`, `bitrate.rs` (phase 3).

## Public API
- `transport`: `Transport` (`start`, `local_addr`, `stats`, `shutdown`), `TransportConfig` (`host`,
  `client`, `loopback`), `LaneCapacities` (`get`, `set`), `Lanes` (`take_tx`, `take_rx`).
- `lane`: `SLOT_LEN` (1232), `TxLane` (`try_reserve`), `TxSlot` (`buf`, `datagram`, `commit`, `seal_commit`),
  `RxLane` (`try_recv`, `recv_timeout`, `attach_current_thread`), `RxDatagram` (`data`, `peer`, `at`, `open`).
- `keys`: `StaticKeyPair` (`generate`; serde hex), `ClientIdentity`, `random_pin`, `random_session`.
- `client_store`: `default_dir`, `load_or_create_identity`, `load_known`, `save_known`, `remember`, `forget`.
- `control_link`: `ControlLink` (`new`, `set_keys`, `send`, `send_sealed`, `accept`, `exchange`), `Wire`, `Incoming`.
- `pairing`: `PairedDevice`, `KnownHost`, `Now`, `PromptView`, `PairError`, `PROMPT_TIME` (120 s), `MAX_ATTEMPTS` (3);
  `host::HostPairing`; `client::{PairContext, request, closed_by_host, submit, start, finish}`.
- `session`: `client::{connect_secure, SecureSession (ping, bye; `mode`, `codec`), init, finish}`; `host::HostSessions`;
  `noise::{NOISE_PATTERN, prologue, derive}`; `PING_EVERY` (1 s), `SESSION_TIMEOUT` (10 s).
- `endpoint`: `HostControl` (`new`, `on_datagram`, `tick`, `prompt`, `cancel_prompt(tx)`, `paired`, `set_paired`,
  `end_session`, `end_all`: sealed `Bye` x3), `HostControlConfig` (`policy`), `EndpointEvent` (`Paired`, `SessionStarted { keys, mode, codec, .. }`, `SessionEnded`).
- `mode_policy` (spec 0020): `ModePolicy { max_fps, codec }` (`welcome`, `choose_mode`, `choose_codec`): mode even,
  ≤ 3840 x 2160, refresh ≤ `max_fps`; else the largest fitting display; `auto` codec = HEVC if offered.
- `stats`, `socket`, `resend` (`ResendTimer`, `HELLO_RESEND` 250 ms, `HELLO_GIVE_UP` 10 s), `handshake`, `error::NetError`.
- `discovery`: `Advert`, `SERVICE_TYPE`, `CAPS_V1`; `Advertiser` + `AdvertiserConfig`; `Browser` + `BrowserConfig`,
  `ProbeTargets`, `OnChange`, `DiscoveryEvent`; `HostList`, `HostEntry`, `Source`, `EXPIRY` (5 s); `LocalNet`; `wire`.

## Notes
- Each lane has one owner thread on each end. An `RxDatagram` is valid until the next receive on its lane.
- Send priority: Control, Input, Feedback, Audio, Cursor, then Video in batches of 32.
- The recv thread routes on bytes 0-1 only; consumers decode. Filtering by session peer is the caller's job.
- `SIO_UDP_CONNRESET` is off: without it a closed peer port breaks the next `recv_from` on Windows.
- One `ControlLink` per peer for its whole life: frame numbers only grow, a new link to the same address is
  dropped as late. Sealed and plaintext messages reassemble separately.
- Host commits a session only on the client's first sealed `Ping` (Noise KK message 1 can be replayed).
- snow features: `risky-raw-split` (keys from the split), `use-chacha20poly1305` (snow 0.10 import bug).
- Discovery: DNS-SD callback contexts that Windows may still use after a cancel are leaked on purpose.
