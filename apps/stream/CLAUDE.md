# mixlink-stream

> Module card. Keep <= 50 lines. Update when the public API changes.

**Purpose:** Stream window process (ADR-0001) spawned by the client launcher: `winit` window, the secure session
with the host (spec 0020), D3D11VA decode and present with the host cursor on a video thread, keyboard + mouse (0021).
**Depends on:** mixlink-core, -proto, -net, -codec, -render, -input.
**Hot path:** `stream-net` receive, decode + present on `video`, `stream-input` (keys / mouse out).

## Layout
- Library (`lib.rs`, used by the binary and `tests/no_alloc.rs`): `args.rs` (`Mode`: `--connect` / `--play`),
  `au_queue.rs` (SPSC AU queue, `AuInfo`), `frame_queue.rs`, `feeder.rs` (file → queue at `--fps`), `stats.rs` (`LATENCY_SPANS`,
  `latency_summary`), `video.rs` (video thread); each with `<name>_tests.rs`. `input/` (0021): `sender.rs` (`InputTx`,
  `Schedule`: +4 / +16 ms, 100 ms while held), `shortcuts.rs`, `capture.rs`, `mode.rs`, Windows `keys.rs`, `thread.rs`.
- `net/` (spec 0020): `session.rs` (`run`: `connect_secure`, then one loop over control / video / cursor lanes,
  ping 1 s, 10 s silence = lost), `lanes.rs` (`Media`: open, reassemble, AU queue, cursor, keyframe requests),
  `receive.rs` (`VideoReceiver`: drop non-key frames after a loss), `cursor_state.rs` (net → video thread,
  atomics + shape ring, shape requests), `feedback_tx.rs` (`FeedbackTx`, `Rtt` median of 16), `drop_knob.rs`,
  `reasons.rs`; each with tests (`session_tests.rs` runs a real `HostControl` on loopback).
- Binary: `main.rs` (event loop + stdin thread), `app.rs` (`StreamApp` window lifecycle), `connect.rs` (`Hello` from
  the monitors, keys from `client_store`, thread `stream-net`), `ipc.rs` (+ tests), `window_input.rs`
  (capture, mouse modes), `platform/window.rs`, `strings.rs`.

## Public API (process contract)
- `mixlink-stream --connect <host id> --addr <ip:port> [--drop video=0.1,cursor=5,input=5]` (launcher; `--drop` = dev loss, %).
- Stdout: `StreamToLauncher` JSON lines only (IPC v2) — `Connecting`, `Connected { width, height, refresh_mhz,
  codec }`, `Ready` after the first present, `Closed { reason, latency }` before exit (`latency` in connect mode).
  Reasons: `User`, `Stopped`, `Error`, `Rejected { busy | badDisplay | version }`, `HostEnded { userClosed |
  shutdown | hostError | timeout }`, `ConnectionLost`, `HostUnreachable`, `NotPaired` (the host is forgotten here).
- Stdin: `LauncherToStream::Stop`; EOF = launcher gone → closes with reason `Stopped` (not in dev runs: `--play`, or
  `--connect` with `--drop`).
- Ctrl+Alt+Shift + F fullscreen, + Z release / take back input (or click), + Q close, + L lock the host (Win+L locks here).
- Exit code 0 after `Closed`; non-zero only if the event loop fails to start or the arguments are bad (2).
- Dev: `mixlink-stream --play <file.h264|.hevc> [--fps 120] [--loop] [--measure]` (spec 0013); stats on stderr
  at close. Test files: `scripts/make-test-stream.sh`.

## Notes
- Never print to stdout outside `ipc::send`; logs go to stderr (`RUST_LOG`).
- Release builds use the GUI subsystem (no console); piped stdio still works.
- The video thread owns the `Presenter` and `Decoder` (opened once the codec is known: `Welcome` or `--play`);
  the winit thread only passes sizes. It decodes every queued AU as it comes; a cursor change
  re-presents the last frame. The cursor is drawn only while its shape is loaded. It presents only when the
  swapchain's frame-latency waitable is signaled (never blocks in `Present`), decoding meanwhile; decoded
  frames wait in `FrameQueue` (3, oldest first; half a second of backlog skips one: `pop_paced`).
- Latency spans: host capture / encode / send (`FrameMeta`), network = RTT / 2, reassemble, queue, decode,
  present, total; no clock sync between the PCs.
- The feeder starts after the first present (decoder open), else the first keyframe is lost.
- Needs the FFmpeg DLLs next to the exe (`build.sh release` copies them into `dist/`).
- Input (0021): captured = focused and not released. Host cursor shown → absolute (drawn at the local pointer,
  `LocalPointer`); hidden 150 ms, no button, keys quiet 1 s → relative (`ModePicker`). Keys: hook `input-hook`, else window.
