# Phase 1 — MVP LAN

**Goal:** a build to test on two real PCs on one LAN: the client sees and hears the host,
and controls it with keyboard, mouse and gamepad, all from desktop apps (no terminal).
**Done when:** a game on the host is playable from the client over wired LAN, with per-stage
latency reported, on a host with a monitor and on a headless host. Tuning to the full budget is phase 3.

UI (ADR-0001): host = Tauri tray app; client = Tauri launcher (React + TS) that spawns the separate
`mixlink-stream` process (`winit` + D3D11 flip-model swapchain). Video never renders in a webview.
Host pipeline lives in the `mixlink-svc` service (ADR-0005); drivers per ADR-0004/0006/0007.
Spikes (S1–S5) come first: each proves one risk with a throwaway prototype and a short report in
its spec; a failed spike switches to the fallback its ADR names before dependent rows start.

| ID | Task | Crates | Needs | Spec | Status |
|---|---|---|---|---|---|
| T1 | App shells: Tauri launcher + host tray, `mixlink-stream` empty window, launcher↔stream IPC, npm in gate/CI | host, client, stream, render | — | [0002](../specs/0002-app-shells.md) | done |
| T14 | Spike S1: UMDF driver signed by a per-machine cert installs and loads on Win11 24H2, Secure Boot on, no test-signing | drivers | — | [0003](../specs/0003-spike-umdf-signing.md) | done |
| T15 | Spike S2: IddCx swapchain frame → shared texture + fence → another process; latency vs DDA | drivers, capture | T14 | [0004](../specs/0004-spike-iddcx-frames.md) | done |
| T23 | Spike S2b: cut the IddCx frame-path tail (driver copy priority, zero-copy ring), re-measure vs DDA with the S2 harness | drivers, capture | T15 | [0005](../specs/0005-spike-iddcx-tail.md) | done |
| T16 | Spike S3: UMDF HID keyboard, mouse, gamepad accepted by Windows and games; report latency | drivers, input | T14 | [0006](../specs/0006-spike-vhid.md) | done |
| T17 | Spike S4: NVENC/AMF/QSV encode and WASAPI loopback from a Session 0 service | codec, audio | — | [0007](../specs/0007-spike-encode-audio-session0.md) | done |
| T18 | Spike S5: Apex Legends (EA Javelin) and Elden Ring (EAC) launch to the main menu with our drivers installed (gameplay: user warning only) | drivers | T15, T16 | [0008](../specs/0008-spike-anticheat-launch.md) | done |
| T2 | Core types: ids, config, errors, monotonic clock, per-stage timestamps | core | — | [0009](../specs/0009-core-types.md) | done |
| T3 | Protocol: packet header + channels (incl. cursor), handshake with client displays, packetizer / reassembler (payload ≤ 1200 B) | proto | T2 | [0010](../specs/0010-protocol-packets.md) | done |
| T4 | UDP layer: socket setup, send/recv threads, SPSC queues | net | T3 | [0011](../specs/0011-udp-transport.md) | done |
| T19 | Service shell: `apps/svc` as service + `--console`, install/uninstall, named pipe to host tray | svc, host, core | T1, T2, T17 | [0012](../specs/0012-service-shell.md) | done |
| T20 | Driver install tooling: per-machine cert, sign catalog, install/uninstall `vdisplay` + `vhid`, WDK build in gate/CI | drivers, svc | T14, T19 | [0014](../specs/0014-driver-install-tooling.md) | done |
| T21 | Virtual display: add/remove monitor in client mode, stable EDID per client, render adapter, topology save/restore, monitor before login | drivers, svc | T15, T20 | [0015](../specs/0015-virtual-display.md) | done |
| T22 | Virtual HID driver: keyboard, mouse (relative + absolute), virtual Xbox pads; IOCTL report feed | drivers, input | T16, T20 | [0016](../specs/0016-virtual-hid.md) | done |
| T5 | LAN discovery (mDNS + broadcast fallback) in the service, host list in the launcher | net, svc, client | T4, T19 | [0017](../specs/0017-lan-discovery.md) | done |
| T6 | Pairing: PIN on host tray, entry on client, X25519 + AES-128-GCM session | net, proto, svc, host, client | T5 | [0018](../specs/0018-pairing-secure-session.md) | done |
| T7 | Capture from the IddCx shared texture (DDA fallback: T24) → NVENC encode via FFmpeg, GPU zero-copy | capture, codec | T2, T21, T23 | [0019](../specs/0019-capture-encode.md) | done |
| T8 | D3D11VA decode via FFmpeg → present in the stream window | codec, render | T1, T2 | [0013](../specs/0013-d3d11va-decode-present.md) | done |
| T9 | End-to-end video host→client, virtual monitor in client mode, cursor channel, per-stage latency report | svc, client, stream | T6, T7, T8 | [0020](../specs/0020-video-end-to-end.md) | wip |
| T10 | Keyboard/mouse: Raw Input + pointer lock on client, virtual HID on host (SendInput fallback), redundant packets | input, svc, stream | T9, T22 | — | todo |
| T11 | Audio: WASAPI loopback in the service → Opus (5 ms) → playback | audio, svc, client | T6, T17 | — | todo |
| T12 | Gamepad: XInput on client, virtual Xbox pads from T22 on host (ADR-0009), rumble back to the client | input, svc | T10, T22 | — | todo |
| T13 | FEC + basic feedback channel so real LAN/Wi-Fi loss is survivable | proto, net | T9 | — | todo |
| T24 | DDA fallback capture: console-session helper, shared texture + fence to the service, used when the IddCx frame share is unavailable | capture, svc | T7 | — | todo |
