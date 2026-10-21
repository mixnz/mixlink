# 0001 — Tech stack

**Status:** accepted · 2026-10-21

## Context
Need near-zero-latency video/input streaming, Windows gaming PCs first, codebase written
mostly by AI agents (the compiler should catch as many mistakes as possible).

## Decision
- **Language:** Rust (Cargo workspace). Strict compiler suits AI-written code; zero-cost FFI.
- **OS:** Windows 10/11 first for host and client. Other clients (macOS/Linux/Android) later.
- **Capture:** Windows.Graphics.Capture; DXGI Desktop Duplication fallback.
- **Video:** H.264 / HEVC / AV1 hardware encoders. Start with FFmpeg (`ffmpeg-next`,
  D3D11 hw frames) for one code path; move to direct NVENC/AMF SDK if profiling shows overhead.
- **Decode/present:** D3D11VA + D3D11 flip-model swapchain.
- **Audio:** WASAPI loopback + Opus (5 ms frames).
- **Input:** Raw Input / XInput on client; SendInput + virtual gamepad on host.
- **Network:** custom UDP protocol (ADR-0002), `mdns-sd` discovery,
  `reed-solomon-erasure` FEC, `aes-gcm` + `x25519-dalek` crypto.
- **UI:** Tauri v2 for both apps — client launcher (device list, pairing, settings) and host tray
  (PIN, paired devices, status). Frontend React + TypeScript (strict) + Vite, npm, in `apps/<app>/ui/`.
  UI reaches Rust only through Tauri commands/events in the app crate; media crates never depend on Tauri.
- **Stream window:** separate process `mixlink-stream` (`apps/stream`), `winit` + D3D11 flip-model swapchain:
  decode, present, input capture. The launcher spawns it on Connect; webviews are closed while streaming,
  so video never passes through a webview.
- **Concurrency:** dedicated threads + `rtrb`/`crossbeam` queues on hot path; `tokio` control plane only.

## Consequences
- Windows-only APIs isolated in `platform/windows` so other clients can be added later.
- FFmpeg DLLs shipped with the app (LGPL build).
- Node LTS + npm are build requirements; gate and CI run the frontend steps. WebView2 required on Windows.
- Launcher ↔ stream process need a local IPC contract (defined in the T1 spec).
