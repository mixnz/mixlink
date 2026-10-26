# Status

> Replace content each update; keep ≤ 80 lines. History lives in git, not here.

**Updated:** 2026-10-26
**Phase:** 1 — LAN MVP

## Done
Details live in each spec; one line per item here.
- Phase 0: repo rules, architecture docs, gate (`scripts/gate.sh`), CI on dispatch (`scripts/ask-ci.sh`),
  `/execute` `/land` `/watch-ci`, Cargo workspace (toolchain 1.99.1). ADRs 0001–0009 accepted.
- T1 app shells (0002): Tauri launcher + host tray, `apps/stream` winit + D3D11 `Presenter`, launcher ↔
  stream JSON-lines IPC, npm workspace with `@mixlink/i18n` (`en` only); `build.sh release` → `dist/`.
- T14 spike S1 (0003): ADR-0004 holds; `drivers/build.ps1` (NuGet WDK `10.0.28000.2526`), per-machine signing.
- T15 / T23 spikes S2 / S2b (0004 / 0005): ADR-0006 holds; IddCx p99 256 µs vs DDA 422 µs with MMCSS +
  shared surface. Build `vdisplay` for IddCx 1.4; the virtual monitor must be the main display.
- T16 spike S3 (0006): ADR-0007 holds with changes; `drivers/vhid` works in games (pad via Steam Input,
  ViGEm for XInput-only). Measure input elevated (UIPI).
- T17 spike S4 (0007): ADR-0005 holds (amended: encode priority); NVENC p99 3.15 ms in Session 0 with
  levers; QSV via decoder-bind pool; loopback gives 10 ms packets. FFmpeg 8.1 in `C:\tools\ffmpeg`.
- T18 spike S5 (0008): EA Javelin and EAC titles launch with our drivers; warning in `docs/compat/anti-cheat.md`.
- T2 core types (0009): ids, QPC clock (`Ticks`), `StageStamps`, host / client config.
- T3 protocol (0010): 16 B header v1, 6 channels, packetizer / reassembler, control handshake, cursor messages.
- T4 UDP transport (0011): `mixlink-net` socket, send / recv threads, per-channel lanes, `Hello` resend;
  `tools/net-probe`. Wired 0 % loss, RTT p99 0.76 ms (laptop Best performance) to 4.1 ms (power saving).
- T19 service shell (0012): `mixlink-svc` as service `MixLink` or `--console`, `install` / `uninstall`,
  pipe `\\.\pipe\mixlink-svc` (ACL, strict lines, `core::svc_ipc`); host tray shows the service link state.
- T8 decode + present (0013): FFmpeg D3D11VA on the presenter's device, NV12 shader pass, video thread
  (newest frame wins), `mixlink-stream --play` dev mode, `scripts/make-test-stream.sh` (BT.709). Decode p99
  at 1440p120: main PC 1.4–1.5 ms, laptop iGPU 1.7–2.3 ms. FFmpeg in CI (`scripts/ci-ffmpeg.sh`) and `dist/`.
- T20 driver tooling (0014): `mixlink-svc drivers install | uninstall | status` (per-machine cert, catalog check,
  no prompt; install removes what is there first; exit 3 when a program holds `vhid`); `vdisplay` isolated
  (DIRID 13); driver build in full gate, CI `drivers` job, `build.sh release` (`dist/drivers/`). Two machine runs pass.
- T21 virtual display (0015): `vdisplay` has no monitor until the service's `ADD_MONITOR` IOCTL (≤ 4, handle
  close departs them); stable EDID per client; `session::begin` / `end` save, switch (`virtualOnly` / `extend`,
  `host.json`), restore the layout; in Session 0 through a console-session helper (`display-agent`, CCD is
  denied there); service start restores a crash-left layout and adds a headless 1080p60 monitor. Main PC runs
  pass (spec 0015 → Results), incl. headless cold boot.
- T22 virtual HID (0016): only the service (SYSTEM / Admins) feeds `vhid` through a control node; feeder exit =
  everything released; NKRO keyboard, relative + absolute mouse reach UAC and the lock screen. Pads are virtual
  Xbox Series pads (ADR-0009): software devices `SWD\MixLinkPad\000N` per slot, inbox `xinputhid`, XInput
  index = slot, Steam with no setup. `input pad | pad-add | pad-remove | probe`. p99: kbd 0.69, abs 0.43, pad 0.47 ms.

## In progress
- T21 open rows (spec 0015): frame grab of login / lock screen on the headless monitor (T7 harness), power cut
  during a session, iGPU + dGPU host.
- Main PC: drivers from `target\drivers` (T22 runs end with `drivers uninstall`), MixLink service removed.
- Main PC and laptop: `MixLinkS4` removed; main PC: S5 drivers removed (`RESULT CLEAN`). Test
  machine: ViGEmBus may still be installed (Settings → Apps). S2b clean-up (spec 0005 → Verdict) not confirmed.

## Next
- Manual checks left from T1: host tray Open/X/Quit, stream resize with debug layer, `npm run dev:host`.
- 60 Hz clients: `Present` blocks the video thread at vblank, AUs wait 2–6 ms before decode (spec 0013 →
  Latency impact). Use the frame-latency waitable object in T9 or phase 3.
- Roadmap phase 1: T5 (discovery in the service), T7 unblocked. Needs a roadmap row: physical monitor
  plugged in while headless is main (login prompt stays on the virtual monitor).
- Spec 0014 open row: tampered package with the remove-first install. (Steam open on a pad: `drivers
  uninstall` passes, pads unplug, spec 0016.)
- T12 creates a pad per client gamepad; rumble (report 0x03) is kept per pad, not yet sent to the client.
- Inputs for later rows: T7 ← 0005, 0007 Verdict (encode levers, QSV pool), 0015 (frame share: only the first
  running swapchain publishes, replace with a share per monitor); T9 ← 0015 (`session::begin` / `end` per client
  session, HDR row later); T11 ← 0007 (10 ms loopback, silence fill); T22 / T12 ← 0006 Verdict; T9 decides cursor
  shape recovery.
- Phase 3 notes: keep CPU / NIC awake while streaming, pacing, measure all power modes (spec 0011).

## Open questions
- Roadmap row for the language switch (EN/VI setting + `vi` strings) not added yet.
- CI pins a BtbN FFmpeg autobuild URL (`ci.yml` `FFMPEG_URL`); BtbN prunes old ones, so bump it when it 404s.
  The CI FFmpeg step has not run yet (no dispatch in T8).
