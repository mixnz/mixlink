# 0020 — End-to-end video, virtual monitor per session, cursor channel

**Status:** in-progress
**Date:** 2026-10-28
**Roadmap:** T9
**Plan:** [0020-video-end-to-end](../plans/0020-video-end-to-end.md) (local only, written by writing-plans skill)
**Crates touched:** `drivers/vdisplay`, `crates/core`, `crates/proto`, `crates/net`, `crates/capture`, `crates/render`, `apps/svc`, `apps/stream`, `apps/client`

## Goal
From the launcher, Connect on a paired host opens the stream window and shows the host's desktop on a
virtual monitor in the client's mode, with the host cursor drawn by the client, and reports per-stage
latency for the session. No terminal on either PC.

## Scope
- In:
  - Host: a client session (`SessionStarted` / `SessionEnded`, spec 0018) adds / removes the virtual monitor
    (spec 0015 `session::begin` / `end`) and starts / stops capture → encode → packetize → sealed send
    (spec 0019 `run_loop` with a network sink).
  - Mode choice in `Welcome` (replaces the echo in `net::endpoint::welcome`), codec choice (H.264 / HEVC).
  - Sealed media lanes: video and cursor (host → client), feedback (client → host).
  - Feedback body v1: keyframe request (lost frame) and cursor shape request (unknown `shape_id`).
  - Cursor channel: IddCx hardware cursor in `vdisplay`, cursor share to the service, cursor messages on the
    wire (`proto::cursor`), client draws the cursor over the video, shape recovery.
  - Client present that never blocks (frame-latency waitable), so steady 60 fps video stays smooth on a 60 Hz client.
  - Client: launcher Connect → `mixlink-stream --connect`; the stream process owns the secure session,
    receive, reassembly, decode / present (spec 0013); IPC v2 with connect states and close reasons.
  - Per-stage latency: host stage times ride in each video frame; the client adds network and its own stages;
    summary in the stream log and in the launcher after the stream ends.
  - Dev loss knob: `mixlink-stream --connect … --drop <video|cursor>=<percent>` drops received datagrams
    before opening them (hidden switch, for the loss criteria below; no loss simulator exists yet).
- Out:
  - Input (T10), audio (T11), gamepad (T12), FEC and bitrate from feedback (T13), DDA capture (T24).
  - Pacing, power settings: phase 3.
  - Fullscreen / pointer lock in the stream window: T10. The window keeps T1 behaviour (resizable, video
    letterboxed); the local Windows cursor stays visible.
  - More than one client at a time (the host answers `Busy`, spec 0018), HDR, AV1, QSV / AMF.
  - Live latency overlay in the stream window: phase 3.

## Design

### Data flow
```
client: launcher Connect → spawn mixlink-stream --connect <host-id> --addr <ip:port>
  stream-net: connect_secure(Hello + codecs) → Welcome(mode, codec) → sealed Ping every 1 s
host: svc-session: SessionStarted → svc-display: begin(client, mode) → spawn svc-video + svc-cursor
  svc-video: IddSource frame → NVENC → [FrameMeta | AU] → packetizer → seal → Video lane
  svc-cursor: CursorSource update → Position / Shape → seal → Cursor lane
client: stream-net: Video lane → open → reassemble → AuQueue → video thread: decode → present
        Cursor lane → cursor state → video thread draws cursor on present (also with no new frame)
end: Bye / 10 s silence / error → svc-session: SessionEnded → stop threads → svc-display: end → restore
```

### Host: session lifecycle (`apps/svc`)
- `svc-display` (spec 0015 thread, owns the `VDisplay` handle and the headless monitor) becomes a
  request loop: `DisplayRequest::Begin { client, mode, reply }` / `End { active, reply }` over an mpsc
  channel. Only this thread calls `session::begin` / `end`; the topology comes from `host.json`.
- `svc-session` on `SessionStarted` starts the stream (`apps/svc/src/stream/`) and never blocks on it:
  `svc-video` sends `Begin` and waits for the monitor (a topology switch can take seconds), then starts
  `svc-cursor` and captures. On `SessionEnded` `svc-session` sets the stream's stop flag; `svc-video` stops
  `svc-cursor`, sends `End`, and hands the lanes back over a channel that `svc-session` polls. A new
  session waits (stays uncommitted, client resends) until the lanes are back.
- A stream error (monitor not added, no NVENC, capture lost for > 2 s) ends the session from the host:
  new `HostControl::end_session(device, tx)` sends sealed `Bye { reason: HostError }` (3 copies, as the
  pairing close notice) and returns `SessionEnded`. Service stop sends `Bye { Shutdown }` the same way.
- Lanes: the host `Transport` (session port 47801) already has every channel. `svc-session` keeps Control;
  Video tx and Feedback rx move to `svc-video`, Cursor tx to `svc-cursor`, and come back when they join.
  `EndpointEvent::SessionStarted` gains the `SessionKeys` and the client address, so each thread builds its
  own `Sealer` / `Opener` for its channel (`SessionKeys::sealer(role, channel)`).
- One session at a time; `svc-session` refuses a `Begin` while one runs (cannot happen: `Busy`).

### Host: mode and codec choice (`Welcome`)
- `welcome` moves from `net::endpoint` to a `ModePolicy` the service passes to `HostControlConfig`.
- Mode: the client's `stream_display` mode if width, height even, ≤ 3840 x 2160; otherwise the largest mode
  from the client's display list that fits; none fits → `BadDisplay`. A refresh above `video.max_fps` is capped
  to it (a 165 Hz screen streams at 120 Hz).
  The virtual monitor is created with exactly that mode, so the EDID's preferred mode is it.
- `Hello` gains `codecs: [h264, hevc]` (decoders the client has; default `[h264]` when missing, so older
  clients still parse). `host.json` `video.codec`: `auto` (default: HEVC if the client has it, else H.264),
  `h264`, `hevc`. `Welcome` gains `codec`.
- Encoder: `video.bitrate_kbps` (default 50 000), fps from the monitor's `RefreshMhz` (spec 0019 Results),
  rebuilt on `Reopened` as in `capture try`. Thread levers: MMCSS + priority class, no GPU priority.

### Host: video send (`svc-video`)
- `NetSink: EncodeSink<IddSource>`: encode → write `FrameMeta` + AU into one preallocated 4 MiB buffer →
  `Packetizer(Video)` → for each fragment `TxLane::try_reserve` → `seal_commit` to the client. Frame id
  +1 per AU; `Flags::KEYFRAME` on IDRs. A full lane drops the rest of the AU → `Delivery::Dropped` → next IDR.
- `FrameMeta` (`crates/proto/src/video.rs`, 16 B LE, before the AU in the message): `version`, `flags`,
  `capture_us` (driver acquire → frame ready), `encode_us`, `send_us` (encode end → last fragment queued),
  as u32 / u16 fields; the plan fixes the layout with size asserts.
- Each loop turn reads the Feedback rx lane with `try_recv` (no wait): `KeyframeRequest` → next frame IDR,
  at most one forced IDR per 50 ms; `CursorShapeRequest` → an atomic flag for `svc-cursor`.
- `run_loop` gets the session stop flag; no deadline (`Instant` far future). At end the host logs its spans
  (capture, encode, send p50 / p99), hold timeouts and ring fallbacks (spec 0019 asks T9 to recheck them).

### Feedback channel v1 (`crates/proto/src/feedback.rs`)
- One packet per message, byte 0 kind: 1 `KeyframeRequest { last_good: FrameId }`, 2
  `CursorShapeRequest { shape_id: u32 }`. Fixed LE layout like `cursor.rs`. T13 adds kinds; unknown kinds
  are dropped.
- Client sends `KeyframeRequest` when a video frame is given up by the reassembler (a newer frame
  completed first, or frame ids skip) or the decoder fails, then at most every 100 ms until a keyframe
  arrives. Until then it drops non-key frames (they would decode to garbage).

### Cursor: driver (`drivers/vdisplay`)
- In `EvtIddCxMonitorAssignSwapChain`: `IddCxMonitorSetupHardwareCursor` with `AlphaCursorSupport = TRUE`,
  `ColorXorCursorSupport = IDDCX_XOR_CURSOR_SUPPORT_EMULATION` (Microsoft's advice when the cursor goes on
  its own channel), `MaxX = MaxY = 256`, and an auto-reset event. From then on Windows no longer draws the
  cursor into the frames.
- A cursor thread per swapchain waits on that event (and the swapchain's stop), calls
  `IddCxMonitorQueryHardwareCursor` into a 256 KiB buffer, and publishes to `Global\MixLinkCursor-<n>`
  (n = connector, same SDDL as the frame share) + auto-reset event `Global\MixLinkCursorReady-<n>`.
- Cursor header v1 (magic `MXC1`), mirrored in `crates/capture` with size / offset asserts: seqlock `Seq`
  (odd while writing), `Visible`, `X`, `Y` (image top-left), `ShapeId`, `ShapeType`, `Width`, `Height`,
  `Pitch`, `XHot`, `YHot`, `ShapeBytes`, then the pixel area. The thread stops on unassign (`Magic = 0`).

### Cursor: service and wire
- `mixlink-capture::CursorSource::open(connector)` → `next(timeout) -> Option<CursorUpdate<'_>>`: position
  (hotspot = `X + XHot`, `Y + YHot`), visible, shape id, and the shape (header + borrowed pixels) when the
  shape id changed. Copy out under the seqlock, retry on a torn read.
- `svc-cursor`: shape changed → `CursorShapeHeader` message (fragmented) first, then `CursorPosition`
  (one packet) per update. Message ids on the cursor channel +1 per message. Shape request flag set →
  resend the current shape (at most every 100 ms). `ShapeType` maps to `ShapeKind::Color` /
  `MaskedColor`; `Monochrome` stays for DDA (T24).
- Shape recovery (open since spec 0010): the client asks with `CursorShapeRequest` when a position names a
  shape it does not have, and draws nothing until it arrives.

### Client: launcher and IPC (`apps/client`)
- Paired, compatible host row: **Connect** button (i18n keys, `natural-copy`). `start_stream(host_id)`
  spawns `mixlink-stream --connect <host-id> --addr <ip:port>` with the discovery address; one stream at a
  time (as T1).
- IPC v2 (`core::ipc`, `IPC_VERSION` 2): `StreamToLauncher` adds `Connecting`, `Connected { width, height,
  refresh_mhz, codec }`; `CloseReason` adds `Rejected { reason }`, `HostEnded { reason }`,
  `ConnectionLost`, `HostUnreachable`, `NotPaired`. `Closed` carries an optional `LatencySummary`.
  `StreamResult` and the launcher text cover each one.
- After a stream ends the host row shows the last session's latency (p50 / p99 per stage, total), until the
  next Connect.

### Client: stream process (`apps/stream`)
- `--connect` mode next to `--play` (the play feeder stays for dev). Keys and the known host come from the
  client store (`%LOCALAPPDATA%\MixLink`, spec 0018); a host no longer known → `NotPaired`.
- Thread `stream-net` (MMCSS): `Transport` client, `connect_secure` with `Hello` from the window's
  monitors (`stream_display` = the window's monitor). Then a loop on the Video, Cursor and Control rx lanes:
  - Video: open → `Reassembler` (video config, preallocated) → split `FrameMeta` → `AuSender` with
    `Received` = first fragment's `at()`, plus the meta and the last fragment's time.
  - Cursor: open → position into a latest-wins atomic slot; shapes into a 2-slot buffer → video thread.
  - Control: sealed `Ping` every 1 s (keepalive, RTT samples), `Bye` → `HostEnded`; 10 s with no valid
    datagram → `ConnectionLost`; `connect_secure` timeout → `HostUnreachable`.
- Video thread: as spec 0013, with the codec from `Welcome`. It also wakes on a cursor change and
  presents the last decoded frame again with the new cursor (hardware cursor moves make no new frames).
- Present never blocks the video thread: the swapchain has a frame-latency waitable object (max latency 1);
  the thread presents only when it is signaled and keeps decoding meanwhile. Decoded frames wait in a queue of
  two, shown oldest first; a third drops the oldest. Newest-wins (and a blocking `Present`) dropped ~5 frames / s
  of steady 60 fps video on a 60 Hz client: two arrivals in one refresh lost one (testufo judder).
- `mixlink-render`: `CursorOverlay` on the `Presenter`: shape → BGRA texture (created per shape, off
  the per-frame path); drawn after the video pass, scaled with the letterboxed video rect. `Color`:
  alpha blend. `MaskedColor`: mask 0 → opaque colour; mask 0xFF → black pixels skipped, others drawn with
  an invert blend (approximates XOR).

### Latency report
- Spans per frame: host `capture`, `encode`, `send` (from `FrameMeta`); `network` = RTT / 2 (median of
  the last 16 pings); client `reassemble` (first → last fragment), `queue`, `decode`, `present`. `total` =
  their sum. No clock sync; glass-to-glass is not measured here (photo method: phase 3).
- `stats.rs` (spec 0013) gains the new spans and counters: frames, keyframe requests, frames given up,
  cursor shape requests. Summary in the stream log on close and in `Closed`.

## Latency impact
- New per-frame work on the host: header + `FrameMeta` write, ~25–115 fragments sealed (spec 0018: seal
  p99 0.4 µs each) → ≤ 50 µs at 1440p. No allocation: AU buffer, packetizer and lane slots are preallocated.
- Burst send of one AU (≤ 137 KiB at 1440p120 H.264, spec 0019) on 1 Gbit/s ≈ 1.1 ms on the wire; pacing
  is phase 3. Budget check in Results: total p99 at 1440p120 on wired LAN, expected ~6–8 ms (capture 0.4 +
  encode 2.8 + wire 1.1 + network ~0.4 + decode 1.5–2.3 + present).
- Cursor: drawn by the client, so it moves without waiting for capture + encode + decode.
- Measured by the report above, on both host and client logs.

## Acceptance criteria
- [ ] Main PC host, laptop client, wired LAN: Connect in the launcher → desktop shown within 3 s, virtual
  monitor in the laptop's mode (or the fallback mode), H.264 and HEVC both tried (`video.codec`).
- [ ] testufo on the host at the client's refresh: smooth, colours right, no torn frames, for 5 minutes.
- [ ] Cursor: moves with the host mouse, shape changes (arrow, I-beam, resize, busy) are right, the
  cursor is not also in the video; an inverted (XOR) cursor in a text box stays visible.
- [ ] Shape recovery: `--drop cursor=5` → the cursor shape recovers after each request.
- [ ] Loss: `--drop video=1` → picture recovers after each loss (keyframe request), no lasting freeze or
  garbage.
- [ ] Close the window → host layout restored (spec 0015) and the headless monitor back on a headless
  host; launcher shows the latency table.
- [ ] Host service stop or tray → client shows "host ended"; cable pulled → "connection lost" within
  ~10 s, host restores its layout.
- [ ] Unpaired / removed client → `NotPaired`; second client while one streams → `Busy` message.
- [ ] Headless host (`virtual-only`): login screen and lock screen streamed with the cursor.
- [ ] Results: latency table (p50 / p99 per span) at 1440p120 H.264 and HEVC; hold timeouts per session.

## Open questions
- Host change of mode during a session (client moves the window to another monitor): not handled; the
  stream keeps the mode from `Welcome`. A later row may renegotiate.
- If `ColorXorCursorSupport = EMULATION` looks bad (solid border), try `FULL` with the invert blend only.
