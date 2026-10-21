# 0002 — App shells

**Status:** done
**Date:** 2026-10-21
**Roadmap:** T1
**Plan:** [0002-app-shells](../plans/0002-app-shells.md) (local only, written by writing-plans skill)
**Crates touched:** `apps/host`, `apps/client`, `apps/stream` (new), `crates/render`, `crates/core`,
repo root (npm workspace), `scripts/`, CI

## Goal
Both desktop apps open without a terminal: the host sits in the tray, the client launcher
spawns an empty `mixlink-stream` window and gets control back when it closes. Later tasks only
fill these shells with features; no app setup is left for them.

## Scope
- In:
  - Host Tauri v2 tray app with a small React window ("Host ready", empty PIN slot for T6).
  - Client Tauri v2 launcher (React): empty device list (T5 fills it), **Connect (demo)** button.
  - New app `apps/stream` (`mixlink-stream`): `winit` window, D3D11 flip-model swapchain
    cleared to one color, correct on resize.
  - Launcher ↔ stream IPC over the child's stdin/stdout (JSON lines), types in `mixlink-core`.
  - npm workspace for both `ui/` folders; lint + typecheck + build in gate and CI.
  - `build.sh release` ships all three exes in `dist/`, UI embedded (no dev server).
- Out:
  - Fullscreen, exit hotkey, stats overlay, cursor/input capture, tearing/VRR flags.
  - Discovery, pairing, any networking, settings persistence, single-instance lock.
  - Installer, code signing, app icons beyond a placeholder.

## Design

### User flow (the demo)
1. `mixlink-host.exe`: tray icon, menu *Open* / *Quit*. *Open* shows the host window;
   closing it hides to tray (app keeps running). *Quit* exits.
2. `mixlink-client.exe`: launcher window with the empty device list and **Connect (demo)**.
3. Connect → launcher spawns `mixlink-stream.exe` → stream window shows a solid color.
   On `Ready` the launcher **closes its webview window** (ADR-0001) but keeps the process alive.
4. Stream window closed → launcher recreates its window, which shows
   "Stream closed.". Crash / wrong IPC version → message with exit code / reason (copy in plan, ADR-0003 i18n).

### Repo layout
| Path | Content |
|---|---|
| `package.json` (root) | npm workspaces `apps/host/ui`, `apps/client/ui`; devDeps `@tauri-apps/cli`, TypeScript, ESLint, Vite; scripts `lint`, `typecheck`, `build` run across workspaces |
| `tsconfig.base.json`, `eslint.config.js` | strict TS; ESLint `max-lines-per-function: 80` (file-limits) |
| `.nvmrc` | Node major pinned (24, current LTS on dev machine) |
| `packages/i18n/` | `@mixlink/i18n`: keyed strings + named placeholders (ADR-0003), shared by both UIs |
| `apps/<app>/ui/` | Vite + React + TS package (`@mixlink/host-ui`, `@mixlink/client-ui`), output `ui/dist` |
| `apps/<app>/tauri.conf.json`, `build.rs`, `icons/` | Tauri v2 config; `frontendDist = ui/dist`, placeholder icon |
| `apps/stream/` | `src/main.rs`, `src/ipc.rs` (stdin reader / stdout writer), `CLAUDE.md` |

- Tauri CLI is the npm devDependency; no `cargo install tauri-cli`.
- App crates expose a `custom-protocol` feature (`tauri/custom-protocol`), as in the Tauri v2
  template; release builds enable it so the exe loads the embedded `ui/dist`, not `devUrl`.

### IPC contract (`crates/core/src/ipc.rs`)
- Transport: launcher spawns the child with piped stdin/stdout; one JSON object per line (UTF-8,
  `\n`). Stdout of `mixlink-stream` is reserved for IPC; its logs go to stderr.
- Types (serde, `#[serde(tag = "type")]`): `StreamToLauncher { Ready { ipc_version }, Closed { reason } }`,
  `LauncherToStream { Stop }`, `CloseReason { User, Stopped, Error { message } }`, const `IPC_VERSION = 1`.
- Helpers `encode_line` / `decode_line` with a typed error; unknown `type` = error, not panic.
- Rules: stream sends `Ready` once its first frame is presented; `Stop` or stdin EOF
  (launcher died) closes the window; stream sends `Closed` then exits 0.
  Launcher: `Ready` with another version → kill child, report a version mismatch.
  Child exits without `Closed` → report a crash with its exit code.
- Control plane only: never carries media or input; nothing per-frame goes through it.

### Stream app (`apps/stream`)
- Main thread: `winit` event loop, window 1280×720 windowed, title "MixLink".
- IPC thread: blocking line reads on stdin → `EventLoopProxy` user event (`Stop` / EOF).
- Locates nothing itself; the launcher passes no args in T1 (args come with T9).
- All three exes use `#![windows_subsystem = "windows"]` outside debug builds (no console window);
  piped stdin/stdout still work for a GUI-subsystem child.

### Render (`crates/render`)
- `Presenter::new(window handle, width, height)`, `resize(w, h)`, `clear_and_present(rgba)`.
- Window handle via `raw-window-handle` (Win32 HWND only); render does not depend on `winit`.
- `platform/windows/d3d11.rs`: D3D11 device + DXGI flip-discard swapchain (2 buffers),
  `ResizeBuffers` on resize. `windows` crate; each `unsafe` block with `// SAFETY:`.
- Debug builds request `D3D11_CREATE_DEVICE_DEBUG`, falling back without it when the
  Graphics Tools feature is not installed.
- No present loop in T1: redraw on `RedrawRequested` / resize only.

### Client launcher (`apps/client`)
- `src/stream_proc.rs`: finds `mixlink-stream.exe` next to `current_exe()` (works in
  `target/<profile>/` and `dist/`); spawn, reader thread, wait for exit.
- Tauri command `start_stream`, command `last_stream_result` (UI calls it on load, because the
  window is recreated after the stream ends). One stream at a time; Connect is disabled while running.
- `RunEvent::ExitRequested` is prevented while a stream runs so closing the webview does not quit.

### Host (`apps/host`)
- Tauri `tray-icon` feature; window close → hide. No commands yet beyond what the shell needs.

### Gate and CI
- `scripts/check-ui.sh`: `npm ci` if `node_modules` is missing or older than `package-lock.json`,
  then `npm run lint`, `npm run typecheck`, `npm test`, `npm run build`.
- `gate.sh`: full gate and `--cargo lint|test` run `check-ui` **before** cargo (Tauri embeds
  `ui/dist` at compile time; missing dist fails clippy). New mode `--ui` runs only it.
  `--fast` / `--checks` unchanged.
- `check-deps.sh`: add `mixlink-stream` (apps rule: any internal dep).
- `ci.yml` `lint` and `test` jobs: `actions/setup-node` from `.nvmrc` with npm cache.
- `build.sh release`: copy `mixlink-stream` too; build app crates with `custom-protocol`.

## Latency impact
None on the media path. IPC is control plane only (spawn / stop). The render presenter here is
the T8 starting point; its per-frame cost is measured there. Spawn → `Ready` time is logged
at `info` in the launcher (informational, no target yet).

## Acceptance criteria
- [ ] `dist/mixlink-host.exe` started by double-click shows a tray icon; *Open* shows the window,
      closing it hides to tray, *Quit* exits the process. *(manual check pending: tray not reachable by automation)*
- [x] `dist/mixlink-client.exe` shows the launcher with an empty device list and Connect (demo),
      loading the embedded UI with no dev server running.
- [ ] Connect opens the stream window with a solid color; the launcher window closes; resizing
      the stream window keeps the color filling it with no D3D11 debug-layer errors. *(resize + debug layer: manual check pending)*
- [x] Closing the stream window brings the launcher back with "Stream closed.".
- [x] Killing `mixlink-stream.exe` from Task Manager brings the launcher back with
      "The stream window crashed (exit code N).".
- [x] Killing the launcher process while streaming makes the stream window close by itself (stdin EOF).
- [x] `mixlink-core` unit tests: every IPC message round-trips; unknown `type` and bad JSON return errors.
- [x] A TS function over 80 lines fails `npm run lint`; a type error fails `npm run typecheck`.
- [ ] `npm run dev:client` / `npm run dev:host` start each app with hot reload of the React UI. *(dev:client verified; dev:host + hot reload pending)*
- [ ] Every user-visible string comes from `ui/src/i18n/en.ts` or `src/strings.rs` (ADR-0003). *(except the host window title, still in `tauri.conf.json`)*
- [x] Module cards for host, client, stream, render, core list the new public API; `STATUS.md` updated.

## Open questions
- None blocking. Decisions taken by the author (review while reading): stdin/stdout JSON lines
  for IPC; IPC types in `mixlink-core`; root npm workspace; ESLint in gate; Node pinned via
  `.nvmrc`; render takes `raw-window-handle`, not `winit`.
