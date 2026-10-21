# MixLink — Working Rules for AI Sessions

Low-latency game streaming: **host** (runs the game) streams video/audio to **client**,
client sends input back. LAN + auto-discovery first, internet (NAT traversal) later.
Target: < 10 ms extra latency on wired LAN at 120 fps (see `docs/architecture/latency-budget.md`).

## Read order — load only what the task needs
1. This file (always loaded). Do NOT re-read it.
2. `docs/STATUS.md` — current state and next steps. Read at session start.
3. The `CLAUDE.md` inside the crate/app you touch (auto-loaded when working there).
4. Other docs only via the index `docs/README.md`, only the ones relevant to the task.
- Never scan the whole repo. Use the module map below to jump to the right folder.
- Never read `docs/plans/` unless the current spec links to a plan you are executing.
- Prefer `Grep`/targeted reads over reading whole files; files are small by rule.

## Module map
| Path | Package | Responsibility |
|---|---|---|
| `crates/core` | mixlink-core | Shared types: ids, config, errors, clock. No platform code. |
| `crates/proto` | mixlink-proto | Wire format, packet headers, FEC, encryption framing. |
| `crates/net` | mixlink-net | UDP transport, pacing, bitrate control, LAN discovery, (later) ICE. |
| `crates/capture` | mixlink-capture | Host screen capture into GPU textures. |
| `crates/codec` | mixlink-codec | HW video encode (host) / decode (client). |
| `crates/audio` | mixlink-audio | Loopback capture, Opus encode/decode, playback. |
| `crates/input` | mixlink-input | Client input capture, host input injection. |
| `crates/render` | mixlink-render | Client presentation (D3D11 swapchain). |
| `apps/host` | mixlink-host | Host binary: wires capture→codec→net, tray UI. |
| `apps/client` | mixlink-client | Client binary: device list UI, stream window. |
| `tools/` | — | Dev tools: latency probe, packet-loss simulator. |

Dependency direction (never reverse): `apps → {capture, codec, audio, input, render, net} → proto → core`.
Crates at the same level must not depend on each other; wire them in `apps/`.

## Hard rules
- **Latency first.** In hot paths (per-frame, per-packet, per-input-event):
  no allocation, no blocking I/O, no mutex, no logging above `trace`. Use preallocated
  buffers and lock-free SPSC queues. Mark hot functions with `// HOT PATH` comment.
- Media and input go over UDP only. TCP/QUIC allowed only for control plane if needed.
- Prefer GPU zero-copy (capture texture → encoder) over CPU copies.
- Platform-specific code lives in a `platform/` submodule behind `#[cfg(windows)]`.
- `unsafe` only in `platform/` or FFI modules, each block with a `// SAFETY:` comment.
- File size limits: `docs/conventions/file-limits.md` (code hard cap 400 lines, docs 300).
- Code identifiers, comments, docs: English. Chat with the user: Vietnamese.
- LF line endings everywhere.

## Workflow
- Feature: spec in `docs/specs/` (template `_template.md`) → plan via writing-plans skill
  in `docs/plans/` (gitignored) → link the plan from the spec → implement.
- Accepted decisions live in `docs/decisions/` (ADR). Do not re-open one unless asked.
- Before ending a task: update `docs/STATUS.md` (keep ≤ 80 lines, replace old content,
  do not append history) and the touched crate's `CLAUDE.md` if its public API changed.

## Commands (Git Bash; details in `scripts/README.md`)
- **Gate (must pass before saying a task is done):** `bash scripts/gate.sh`
  (`--fast` = repo checks + fmt only). Fix every ERROR; WARN = consider splitting.
- Build: `bash scripts/build.sh [debug|release]`.
- While iterating, build/test one crate only: `cargo test -p mixlink-<name>`.
- After cloning: `bash scripts/setup-hooks.sh` (pre-commit fast gate, pre-push full gate).
