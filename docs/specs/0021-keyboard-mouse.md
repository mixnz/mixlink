# 0021 — Keyboard and mouse from the client to the host

**Status:** in-progress
**Date:** 2026-10-28
**Roadmap:** T10
**Plan:** [0021-keyboard-mouse](../plans/0021-keyboard-mouse.md) (local only, written by writing-plans skill)
**Crates touched:** `crates/proto`, `crates/input`, `crates/net` (lane only), `apps/stream`, `apps/svc`

## Goal
While the stream window has focus, the client's keyboard and mouse drive the host as if they were
plugged into it: desktop use with the host cursor under the local pointer, and games with mouse-look
(pointer locked, raw deltas). The local Windows cursor is hidden over the video. A lost datagram never
leaves a key or button stuck, and never loses a click.

## Scope
- In:
  - Wire: `Input` channel body v1 (`proto::input`): a loss-tolerant state packet (below), sealed, one
    datagram each, sent on every input event plus timed repeats ("redundant packets" in the row).
  - Client keyboard: a low-level keyboard hook (`WH_KEYBOARD_LL`) while the window is focused and input is
    captured, so Win, Alt+Tab, Alt+F4, Ctrl+Esc reach the host. Scan codes → HID usages (layout-free:
    the host's layout decides the character). Ctrl+Alt+Del and Win+L stay local (Windows reserves them).
  - Client mouse, two modes chosen from the host cursor's `visible` flag (cursor channel, spec 0020):
    - **Absolute** (host cursor visible: desktop, menus): position in stream pixels from the window
      (letterbox removed); the client draws the host cursor shape at the local pointer (no round trip).
    - **Relative** (host cursor hidden: a game took the mouse): pointer confined and hidden, raw deltas
      (winit `DeviceEvent::MouseMotion` = Raw Input), sent as running totals.
    - Buttons 1–5, vertical + horizontal wheel (120 units = one notch).
  - Local Windows cursor hidden over the window in both modes; shown again outside it / when released.
  - Local shortcuts, never sent: `Ctrl+Alt+Shift+Z` releases / recaptures input (hook off, pointer free),
    `Ctrl+Alt+Shift+F` fullscreen (replaces F11, which now goes to the host), `Ctrl+Alt+Shift+Q` closes.
    Released input comes back on the shortcut or a click in the window; focus gained = captured.
  - Focus lost, release shortcut, window closed: the client sends an all-released state first.
  - Host: thread `svc-input` per session (Input lane → open → apply → `vhid` feed), abs mapping to the
    virtual monitor's place on the host desktop, release-all at session end and after 500 ms of silence
    while something is held. The feeder `VHid` moves from `run.rs` into a shared `Arc` for the session.
  - Dev knob `--drop input=<percent>`: the client drops input datagrams before sending (spec 0020 knob).
  - Per-session input stats in both logs (packets, repeats, recovered events, apply time p50 / p99).
- Out:
  - `SendInput` fallback (driver missing): the service is in Session 0 and cannot `SendInput` into the
    console session; it needs a console-session helper like `display-agent`. Row T25.
  - Gamepad (T12), keyboard LEDs / Caps Lock sync, IME, touch / pen, high-resolution wheel, clipboard.
  - Input latency overlay, input → photon measurement: phase 3.

## Design

### Data flow
```
client winit thread: WindowEvent (abs, buttons, wheel) / DeviceEvent (raw dx, dy) → SPSC ring + wake
client stream-input thread: LL hook (keys, on this thread) + ring → InputState (crates/input, pure)
  → InputPacket → seal → Input lane                                       // HOT PATH
  timers: repeat at +4 ms and +16 ms, keep-alive 100 ms while anything is held
host svc-input: Input lane → open → InputApplier (pure) → Report(s) → VHid::feed
host svc-cursor: cursor visible flag → client picks absolute / relative
```
The `Input` `TxLane` is taken in `stream-net`'s `open_link`; after `Welcome` an `InputTx` (lane, sealer,
packetizer, host address) goes over a one-shot channel to `stream-input`, which the winit thread starts
on `Connected`. The lane has one owner on each end, as before (`net` CLAUDE.md). The hook lives on
`stream-input` (its own message loop), so a busy or modal winit thread (window drag) never delays keys.

### Wire: `proto::input` (client → host, fixed little-endian, ≤ 96 B)
| Field | Size | Meaning |
|---|---|---|
| kind | 1 | 1 = state (other kinds later) |
| seq | 4 | packet number; host ignores ≤ last applied |
| mode | 1 | 0 none, 1 absolute, 2 relative |
| buttons | 1 | bits 1–5, after every event below |
| abs x, y | 2 + 2 | stream pixels (absolute mode) |
| rel dx, dy | 4 + 4 | running totals since session start (wrapping i32) |
| wheel, hwheel | 4 + 4 | running totals, 1/120 notch |
| first_event | 4 | number of the first event below |
| count | 1 | 0..=8 events |
| events | 8 × 2 | `(code, down)`: key = HID usage, button = 0xF0 + n |
| keys | 32 | NKRO bitmap of HID usages 0..=255 held after the events |

- Totals instead of deltas: the host applies `total − last_total`, so motion in a lost packet arrives
  with the next one. Events carry the order (a click or key tap lost in one packet is replayed from the
  next, up to 8 events back); the bitmap fixes anything older (more than 8 events in lost packets).
- Repeats are new datagrams with the same `seq`'s content but a fresh crypto nonce; the host's `seq`
  check makes them idempotent.

### Client (`apps/stream`, `crates/input`)
- `crates/input`: `keymap.rs` (set-1 scan code + E0 → HID usage, pure, table test), `client/state.rs`
  (`InputState`: event ring, totals, mode, `release_all`, `packet()`; no heap), `platform/windows/kbd_hook.rs`
  (install / remove the hook on the calling thread; the callback swallows keys while captured and calls
  a thread-local sink; `// SAFETY:` per block). Depends on `proto` for the packet type (allowed direction).
- `apps/stream/src/input/`: `capture.rs` (winit events → `InputState`, letterbox math shared with the
  presenter's rectangle), `mode.rs` (absolute ↔ relative from the cursor flag, with confine / hide /
  show via `Window::set_cursor_grab(Confined)` and `set_cursor_visible`), `sender.rs` (`InputTx`, repeat
  and keep-alive timers), `shortcuts.rs`.
- Cursor prediction: in absolute mode the video thread draws the shape at the local pointer position
  (an atomic written by the winit thread); host positions still drive visibility and shape.
- Captured = focused and not released by the shortcut. Not captured → no hook, pointer free and visible,
  nothing sent but the release state.

### Host (`apps/svc`, `crates/input`)
- `crates/input/src/host/apply.rs`: `InputApplier` (pure, tests with a fake `ReportSink`): seq check,
  new events in order → keyboard / button reports, bitmap reconcile, relative totals → `mouse_rel_reports`
  (wheel notches from the 1/120 remainder), absolute → `mouse_abs_report` through `AbsMap`.
- `AbsMap`: stream pixel → 0..=32767 over the virtual desktop, from the monitor's desktop rectangle and
  the desktop bounds (read after `session::begin`; `virtual-only` = the whole range). First plan task
  checks on the main PC that `vhid` absolute spans the virtual desktop (not only the main display).
- `svc-input` thread (spawned by `stream_on` next to `svc-cursor`, `recv_timeout` on the lane):
  `// HOT PATH`; `Option<Arc<VHid>>` from `run.rs` (no driver → log once, drop input).
- Release-all: session end, stop, and 500 ms without a packet while a key or button is held.

## Latency impact
- Client: event → (mouse: ring + thread wake, ~10–30 µs) → seal → lane; keys skip the hop (the hook
  runs on `stream-input`). No allocation. Raw mouse at
  1 kHz = 1000 datagrams/s of ~110 B: negligible next to video.
- Network: RTT / 2 (spec 0020: 0.65 / 0.75 ms p50 / p99).
- Host: open + apply + feed; spec 0016 feed p99 kbd 0.69, abs 0.43, rel 2.38 ms (Windows' tail).
- Measured: host log per session (receive → feed done, p50 / p99), client log (send cost, repeats).
  Absolute cursor is drawn locally, so pointer feel does not wait for capture + encode + decode.

## Acceptance criteria
- [ ] Typing in Notepad on the host from the laptop: every key, Shift / Ctrl / Alt combos, Win key opens
  Start on the host (not the laptop), Alt+Tab switches host windows; no key stuck after 5 minutes.
- [ ] Desktop mouse: host cursor sits under the laptop pointer at every edge and corner (windowed and
  F11 / `Ctrl+Alt+Shift+F` fullscreen, letterboxed), clicks, double-click, drag, right-click, wheel.
- [ ] Game with mouse-look (a spec 0006 game): pointer locked, no local cursor, camera turns smoothly,
  WASD + mouse buttons work; Esc to the game menu brings the absolute mode back.
- [ ] Focus loss (Alt+Tab out via `Ctrl+Alt+Shift+Z`, clicking another window) with W held → host
  releases W at once; window close with keys held → released.
- [ ] Loss: `--drop input=5` → no stuck key or button after 1 minute of typing and clicking, every
  click lands (counted on the host: recovered events in the log).
- [ ] Cable pulled while holding a key → host releases within 0.5 s.
- [ ] UAC prompt and lock screen on the host are usable from the laptop.
- [ ] `extend` topology: absolute mouse lands on the virtual monitor, not the physical one.
- [ ] Results: input apply time p50 / p99 and repeats per session from the logs.

## Open questions
- (Answered 2026-10-28: `SendInput` fallback is row T25; fullscreen moves to `Ctrl+Alt+Shift+F`; a cursor
  warped by a host program in absolute mode stays at the local pointer until the mouse moves.)
- Windows "hide pointer while typing" on the host flips to relative while typing; a short hold
  (e.g. 150 ms hidden before switching) may be needed. Decide after the first run.
