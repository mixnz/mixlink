# 0011 — UDP transport: socket, send / recv threads, SPSC lanes

**Status:** in-progress
**Date:** 2026-10-24
**Roadmap:** T4
**Plan:** [0011-udp-transport](../plans/0011-udp-transport.md) (local only, written by writing-plans skill)
**Crates touched:** `crates/net`, `tools/net-probe` (new workspace member), workspace `Cargo.toml`

## Goal
One UDP session socket per side that later tasks (T6 pairing, T9 video, T10 input, T11 audio,
T13 FEC) plug into without touching sockets: datagrams go in and come out through
per-channel lock-free lanes, a dedicated send thread and recv thread do the syscalls, and the
client's `Hello` is resent until the host answers. A dev tool proves it between two real PCs.

## Scope
- In:
  - `socket.rs`: open + configure the socket (bind, buffers, read timeout, Windows UDP fixes).
  - `lane.rs`: per-channel SPSC lanes of preallocated datagram slots (`TxLane`, `RxLane`).
  - `transport.rs`: `Transport` owns the socket, the send thread and the recv thread; start / stop.
  - `resend.rs`: `ResendTimer`, pure logic for "resend every X until reply or deadline".
  - `handshake.rs`: client side of the T3 handshake (`Hello` → `Welcome` / `Reject`) on the control lanes.
  - `stats.rs`: per-lane and per-socket counters (relaxed atomics, read as a snapshot).
  - `platform/windows/`: `SIO_UDP_CONNRESET` off, MMCSS on the send / recv threads.
  - `tools/net-probe`: `serve` / `run` dev tool for loopback and two-PC LAN tests.
- Out:
  - Pacing and bitrate control (phase 3): the send thread sends as fast as the lanes fill.
  - DSCP marking: needs qWAVE (`QOSAddSocketToFlow`, admin for custom DSCP); phase 3 measures if it helps.
  - IPv6, multiple peers per socket, NAT traversal (phase 4).
  - LAN discovery and its port 47800 (T5); encryption (T6, slots already have room for the tag); FEC (T13).
  - Host-side session logic (accept `Hello`, build the virtual monitor, `Welcome`): `apps/svc` in T9.
    `net-probe serve` answers `Hello` itself for testing.
  - Batched send / receive (USO / URO, RIO): phase 3, only if measurements ask for it.

## Design

### Dependencies (new, versions checked 2026-10-24)
| Crate | Version | Why |
|---|---|---|
| `socket2` | 0.6.5 | Buffer sizes and socket setup that `std::net::UdpSocket` lacks, without `unsafe` |
| `rtrb` | 0.4.0 | Wait-free SPSC ring (`push` / `pop`); keeps `unsafe` out of a hand-written ring |
| `windows` | 0.62.2 (workspace) | Features `Win32_Networking_WinSock` (`WSAIoctl`), `Win32_System_Threading` (MMCSS) |

### Socket (`socket.rs`, `platform/windows/socket.rs`)
- IPv4 UDP, `bind(addr)`: host `0.0.0.0:session_port` (47801, `core::config::NetConfig`), client `0.0.0.0:0`.
- Receive and send buffers 4 MiB each (one keyframe burst ≈ 3500 datagrams ≈ 4.3 MB must fit).
  The value Windows actually applied is read back and kept in the stats snapshot.
- Read timeout 20 ms: the recv thread checks its stop flag at least that often.
- Windows: `WSAIoctl(SIO_UDP_CONNRESET, FALSE)`. Without it, one ICMP "port unreachable" (peer
  closed) makes the next `recv_from` fail with `WSAECONNRESET`. Only `unsafe` in the crate, in `platform/`.
- The socket is shared by the two threads (`UdpSocket` is `Sync`; one sends, one receives).

### Datagram slots and lanes (`lane.rs`)
- `SLOT_LEN = 1232` = `proto::MAX_DATAGRAM` (1216) + 16 B GCM tag room for T6. A slot is
  `Box<Slot { buf: [u8; SLOT_LEN], len, peer: SocketAddrV4, at: Ticks }>`.
- Each lane has two `rtrb` rings of `Box<Slot>`: **filled** (producer → consumer) and **free**
  (consumer → producer). All boxes are allocated at `Transport::start`; after that slots only
  move between rings (a pointer move), so the hot path never allocates or copies a whole slot.
- `TxLane` (producer side, one owner thread, `Send`, not `Clone`):
  - `try_reserve() -> Option<TxSlot<'_>>`: takes a free slot, or `None` when the lane is full
    (counted `tx_full`; the caller drops or retries, it never blocks).
  - `TxSlot::datagram() -> &mut [u8; MAX_DATAGRAM]` fits `proto::Fragments::next_into` directly,
    so the packetizer writes straight into the slot: one copy, message → slot.
  - `TxSlot::commit(len, peer)`: pushes to the filled ring and wakes the send thread; dropping a
    `TxSlot` without commit returns it to the free ring.
- `RxLane` (consumer side, one owner thread):
  - `try_recv() -> Option<RxDatagram<'_>>` (`data()`, `peer()`, `at()` = recv time from `clock::now()`);
    dropping the `RxDatagram` returns the slot. Feeds `proto::PacketHeader::decode` + `Reassembler::push`.
  - `recv_timeout(Duration) -> Option<RxDatagram<'_>>`: parks the thread until the recv thread
    pushes or the timeout ends. Wake-up uses `std::thread::park` / `unpark` (no mutex: an atomic
    swap, plus a futex-style wake only when the consumer is parked). The consumer's `Thread`
    handle is registered on the first wait, or by `attach_current_thread()`: one thread can then
    serve several rx lanes with `try_recv` + `std::thread::park_timeout`.
- Default capacities (slots per lane, each direction; `LaneCapacities` overrides):

| Channel | Slots | Sized for |
|---|---|---|
| `Control` | 128 | a 64 KiB control message (55 fragments) twice |
| `Video` | 4096 | a 4 MiB keyframe (3496 fragments) |
| `Audio` | 64 | 320 ms of 5 ms Opus packets |
| `Cursor` | 512 | a 264 KiB shape (226 fragments) twice |
| `Input` | 256 | input bursts at 8 kHz mouse polling |
| `Feedback` | 64 | |

  Memory: about 6 MB per direction for video, < 1.5 MB for the rest. Both sides get all six
  lanes in both directions (simple, < 16 MB total); unused lanes are just never taken.

### Threads (`transport.rs`)
- `Transport::start(TransportConfig) -> Result<(Transport, Lanes), NetError>` opens the socket,
  allocates every slot, spawns `mixlink-net-send` and `mixlink-net-recv`, and returns the
  application ends: `Lanes { tx: [Option<TxLane>; 6], rx: [Option<RxLane>; 6] }` with
  `take_tx(Channel)` / `take_rx(Channel)`, so each lane moves to the thread that owns that channel.
- **Send thread** (`// HOT PATH`): drains filled tx rings in priority order
  `Control`, `Input`, `Feedback`, `Audio`, `Cursor`, `Video`, sending each slot with `send_to`
  and returning it to the free ring. At most 32 video datagrams per pass, then it re-checks the
  higher lanes, so audio and input never wait behind a whole keyframe. When every ring is empty
  it parks; `commit` unparks it. Send errors are counted (`send_err`), never retried
  (UDP: a late retry is worse than loss).
- **Recv thread** (`// HOT PATH`): blocking `recv_from` into its own 1500 B staging buffer, then:
  too short for a header, IPv6 source, unknown version / channel byte, or longer than `SLOT_LEN`
  → counted `rx_bad` and dropped. Otherwise copies into a free slot of that channel's rx lane
  (one ≤ 1232 B memcpy, tens of ns), stamps `at`, pushes, unparks the consumer. Lane full →
  `rx_full` (the consumer is too slow; same effect as network loss). The full header check stays
  with the consumer (`PacketHeader::decode`), the recv thread only reads bytes 0–1 to route.
  Peer filtering is the application's job (it knows the session); every datagram carries its `peer`.
- Both threads raise themselves with MMCSS "Pro Audio" (`AvSetMmThreadCharacteristicsW`), as in
  S2b / S4. Best effort: a failure is logged once at start and counted in stats, never fatal.
- Stop: `Transport::shutdown()` (also on `Drop`) sets an atomic flag, unparks the send thread,
  and joins both; the recv thread sees the flag within the 20 ms read timeout. No thread
  outlives the `Transport`.
- `Transport::local_addr()`, `Transport::stats() -> TransportStats` (per-lane `tx_sent`,
  `tx_full`, `rx_recv`, `rx_full`; socket `send_err`, `recv_err`, `rx_bad`, applied buffer sizes,
  `mmcss_ok`).

### Resend timer (`resend.rs`)
- `ResendTimer::new(start: Ticks, interval: Duration, give_up: Duration)`;
  `poll(now: Ticks) -> ResendAction { Send, Wait(Duration), GiveUp }`. Pure, no clock inside,
  unit-tested with fake ticks. First `poll` returns `Send`; the next send is `interval` after the
  last `Send` (no catch-up burst); `Wait` never runs past the give-up point.
- Handshake values: resend every 250 ms, give up after 10 s (`HELLO_RESEND`, `HELLO_GIVE_UP`).
  Kept generic: T6 / later control messages reuse it.

### Client handshake (`handshake.rs`)
- `connect(tx: &mut TxLane, rx: &mut RxLane, host: SocketAddrV4, hello: &ControlMessage,
  cancel: &AtomicBool) -> Result<HandshakeReply, NetError>`, blocking, run on the client's control
  thread (control plane, allocation ok). Packetizes `Hello` with its own `Packetizer` /
  `Reassembler` (control config), sends, waits on `rx.recv_timeout`, resends per `ResendTimer`.
- Accepts only datagrams from `host`; returns `HandshakeReply::Welcome { session, mode }` or
  `Reject(reason)`; `NetError::Timeout` after 10 s; `NetError::Cancelled` if `cancel` is set.
  Every resend uses a new control `FrameId` (the host's reassembler would drop a repeated one as late).
- The host side is not in `net` (T9); `net-probe serve` and the tests answer `Hello` themselves.

### Errors (`error.rs`)
`NetError` (`thiserror`): `Bind { addr, source }`, `SocketOption { option, source }`, `Spawn`,
`Timeout`, `Cancelled`, `Control(proto::ControlError)`, `Packetize(proto::PacketizeError)`,
`UnexpectedReply`. Hot-path failures are counters, not errors.

### Dev tool (`tools/net-probe`, workspace member `mixlink-net-probe`)
- `net-probe serve [--port 47801]`: answers `Hello` with `Welcome`, echoes `Input` pings back on
  `Feedback`, reassembles video and acks each complete frame on `Feedback`; prints its counters
  when the client sends `Bye`, then waits for the next client.
- `net-probe run <host-ip> [--port] [--seconds 30] [--mbps 50] [--fps 120]`: handshake, then
  sends synthetic video frames (size = mbps / fps, packetized, no pacing) and 1000 input pings / s;
  prints handshake time, ping RTT p50 / p99 / max, video frames sent / acked / lost %, own counters.
- Probe-only payloads (input / feedback bodies are not defined until T10 / T13): byte 0 kind,
  `1` ping `{seq: u32, sent: u64 ticks}`, `2` frame ack `{frame: u32}`; little-endian.
- Plain `std::env::args` parsing, `println!` output, English only (dev tool, no i18n). `README.md`
  says how to run it on two PCs, including the Windows Firewall prompt for `serve`.

## Latency impact
- Stages: `Sent` → `Received` (network) plus the lane hand-off on each side.
- Expected: tx lane → `send_to` wake-up on the send thread and `recv_from` → rx lane →
  consumer wake-up each cost one thread wake (tens of µs on Windows) and one memcpy on receive.
  No pacing, so a keyframe leaves as one burst (budget "Packetize + send ≤ 0.5 ms" for the send side).
- Measured by: an integration test sending 10 000 single datagrams over loopback through the
  lanes, one at a time, `at` (rx) minus commit time, printing p50 / p99 / max (`--nocapture`);
  and `net-probe` RTT on the real LAN (main PC ↔ laptop, wired). Results go in this section.

## Acceptance criteria
- [ ] Loopback: datagrams of 0, 1, 1216 and 1232 B on every channel arrive on the right rx lane
      with the right bytes and sender address; a 1 MiB video frame packetized into the tx lane
      reassembles on the other side.
- [ ] Bad datagrams (too short, unknown version or channel, longer than 1232 B) are counted
      `rx_bad` and never reach a lane; a full rx lane counts `rx_full` and the recv thread keeps going.
- [ ] A full tx lane returns `None` from `try_reserve` (counted `tx_full`), never blocks.
- [ ] With the video tx lane holding 4000 datagrams, an `Input` datagram committed afterwards
      leaves before the video backlog is sent (priority + 32-datagram video batches).
- [ ] After sending to a closed port, `recv_from` still works (`SIO_UDP_CONNRESET` off).
- [ ] `TxLane::try_reserve` / `TxSlot::commit` and `RxLane::try_recv` / slot release do not allocate
      (counting-allocator integration test, as in `mixlink-proto`).
- [ ] `shutdown` joins both threads within 100 ms; dropping `Transport` does the same.
- [ ] `ResendTimer` gives `Send` at 0, 250, 500 ms… and `GiveUp` at 10 s (fake ticks).
- [ ] Handshake: with the first two `Hello`s dropped by the test host, `connect` gets `Welcome`
      on the third; a `Reject` is returned as such; no reply → `Timeout`; `cancel` → `Cancelled`;
      a reply from another address is ignored.
- [ ] Loopback one-datagram latency (commit → rx `at`) p99 ≤ 250 µs in release, value recorded.
- [ ] `net-probe` between main PC and laptop on wired LAN: handshake works, 30 s at 50 Mbps / 120 fps
      with ≤ 0.1 % video frames lost; RTT p50 / p99 recorded here (user runs it, two machines).
- [ ] `crates/net/CLAUDE.md` lists the public API; `protocol.md` names the resend values;
      `tools/CLAUDE.md` lists `net-probe`.

## Open questions
- None blocking. Calls made in this spec (review them): `socket2` + `rtrb` as new dependencies
  instead of hand-written `unsafe`; per-channel lanes with fixed send priority; boxed slots moved
  between two rings; `park` / `unpark` wake-ups; MMCSS on both threads; no pacing and no DSCP in
  T4; `net-probe` as a kept workspace tool (not a throwaway spike).
