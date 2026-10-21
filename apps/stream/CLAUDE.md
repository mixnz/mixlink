# mixlink-stream

> Module card. Keep <= 50 lines. Update when the public API changes.

**Purpose:** Stream window process (ADR-0001) spawned by the client launcher: `winit` window, present via
`mixlink-render`; later decode (T8) and input capture (T10).
**Depends on:** mixlink-core (IPC types), mixlink-render.
**Hot path:** present per frame (from T8); T1 only redraws on resize.

## Layout
- `main.rs` (event loop + stdin thread), `app.rs` (`StreamApp` window lifecycle), `ipc.rs` (+ `ipc_tests.rs`),
  `strings.rs` (user-visible text, ADR-0003)

## Public API (process contract)
- Stdout: `StreamToLauncher` JSON lines only — `Ready` after the first present, `Closed { reason }` before exit.
- Stdin: `LauncherToStream::Stop`; EOF = launcher gone → closes with reason `Stopped`.
- Exit code 0 after `Closed`; non-zero only if the event loop fails to start.

## Notes
- Never print to stdout outside `ipc::send`; logs go to stderr (`RUST_LOG`).
- Release builds use the GUI subsystem (no console); piped stdio still works.
