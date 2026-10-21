# Phase 1 — MVP LAN

**Goal:** a build to test on two real PCs on one LAN: the client sees and hears the host,
and controls it with keyboard, mouse and gamepad, all from desktop apps (no terminal).
**Done when:** a game on the host is playable from the client over wired LAN, with per-stage
latency reported. Tuning to the full budget is phase 3.

UI (ADR-0001): host = Tauri tray app; client = Tauri launcher (React + TS) that spawns the separate
`mixlink-stream` process (`winit` + D3D11 flip-model swapchain). Video never renders in a webview.

| ID | Task | Crates | Needs | Spec | Status |
|---|---|---|---|---|---|
| T1 | App shells: Tauri launcher + host tray, `mixlink-stream` empty window, launcher↔stream IPC, npm in gate/CI | host, client, stream, render | — | — | todo |
| T2 | Core types: ids, config, errors, monotonic clock, per-stage timestamps | core | — | — | todo |
| T3 | Protocol: packet header + channels, packetizer / reassembler (payload ≤ 1200 B) | proto | T2 | — | todo |
| T4 | UDP layer: socket setup, send/recv threads, SPSC queues | net | T3 | — | todo |
| T5 | LAN discovery (mDNS + broadcast fallback), host list in the launcher | net, client, host | T1, T4 | — | todo |
| T6 | Pairing: PIN on host tray, entry on client, X25519 + AES-128-GCM session | net, proto, host, client | T5 | — | todo |
| T7 | Capture (Windows.Graphics.Capture) → NVENC encode via FFmpeg, GPU zero-copy | capture, codec | T2 | — | todo |
| T8 | D3D11VA decode via FFmpeg → present in the stream window | codec, render | T1, T2 | — | todo |
| T9 | End-to-end video host→client + per-stage latency report | host, client | T6, T7, T8 | — | todo |
| T10 | Keyboard/mouse: Raw Input + pointer lock on client, SendInput on host, redundant packets | input, host, client | T9 | — | todo |
| T11 | Audio: WASAPI loopback → Opus (5 ms) → playback | audio, host, client | T6 | — | todo |
| T12 | Gamepad: XInput on client, virtual pad on host (driver choice ADR) | input, host | T10 | — | todo |
| T13 | FEC + basic feedback channel so real LAN/Wi-Fi loss is survivable | proto, net | T9 | — | todo |
