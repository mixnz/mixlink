# mixlink-stream

> Module card. Keep <= 50 lines. Update when the public API changes.

**Purpose:** Stream window process (ADR-0001) spawned by the client launcher: `winit` window, D3D11VA decode
and present on a video thread; later network input (T9) and input capture (T10).
**Depends on:** mixlink-core, mixlink-codec, mixlink-render.
**Hot path:** decode + present per frame on the `video` thread.

## Layout
- Library (`lib.rs`, used by the binary and `tests/no_alloc.rs`): `args.rs` (`--play` dev mode), `au_queue.rs`
  (SPSC AU queue), `feeder.rs` (file → queue at `--fps`), `stats.rs` (stage spans), `video.rs` (video thread);
  each with `<name>_tests.rs`.
- Binary: `main.rs` (event loop + stdin thread), `app.rs` (`StreamApp` window lifecycle), `ipc.rs`
  (+ `ipc_tests.rs`), `platform/window.rs` (window handle usable off the event loop thread), `strings.rs`.

## Public API (process contract)
- Stdout: `StreamToLauncher` JSON lines only — `Ready` after the first present, `Closed { reason }` before exit.
- Stdin: `LauncherToStream::Stop`; EOF = launcher gone → closes with reason `Stopped`.
- Exit code 0 after `Closed`; non-zero only if the event loop fails to start or the arguments are bad (2).
- Dev: `mixlink-stream --play <file.h264|.hevc> [--fps 120] [--loop] [--measure]` (spec 0013); stats on stderr
  at close. Test files: `scripts/make-test-stream.sh`.

## Notes
- Never print to stdout outside `ipc::send`; logs go to stderr (`RUST_LOG`).
- Release builds use the GUI subsystem (no console); piped stdio still works.
- The video thread owns the `Presenter` and `Decoder`; the winit thread only passes sizes (`resize`).
  It decodes every queued AU and presents the newest (older ones counted `dropped`).
- The feeder starts after the first present (decoder open), else the first keyframe is lost.
- Needs the FFmpeg DLLs next to the exe (`build.sh release` copies them into `dist/`).
