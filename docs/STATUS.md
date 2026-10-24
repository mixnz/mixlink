# Status

> Replace content each update; keep ≤ 80 lines. History lives in git, not here.

**Updated:** 2026-10-24
**Phase:** 1 — LAN MVP

## Done
Details live in each spec; one line per item here.
- Phase 0: repo rules, architecture docs, gate (`scripts/gate.sh`), CI on dispatch (`scripts/ask-ci.sh`),
  `/execute` `/land` `/watch-ci`, Cargo workspace (toolchain 1.99.1). ADRs 0001–0008 accepted.
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

## In progress
- T19 manual checks (elevated, main PC): spec 0012 Acceptance → the unticked lines (install, recovery,
  tray reconnect, ACL, Ctrl+C in `--console`). Not run by the agent (no elevation).
- Nothing else. Main PC and laptop: `MixLinkS4` removed; main PC: S5 drivers removed (`RESULT CLEAN`). Test
  machine: ViGEmBus may still be installed (Settings → Apps). S2b clean-up (spec 0005 → Verdict) not confirmed.

## Next
- Manual checks left from T1: host tray Open/X/Quit, stream resize with debug layer, `npm run dev:host`.
- Roadmap phase 1: T8, T5 (discovery in the service), T20 (driver tooling) unblocked.
- Inputs for later rows: T20 ← specs 0003, 0008 Verdict (`vhid` held open by Steam blocks uninstall);
  T21 ← 0004, 0005 Verdict, 0008 (main display via `SetDisplayConfig`); T7 ← 0005, 0007 Verdict (encode
  levers, QSV pool); T11 ← 0007 (10 ms loopback, silence fill); T22 / T12 ← 0006 Verdict; T9 decides
  cursor shape recovery.
- Phase 3 notes: keep CPU / NIC awake while streaming, pacing, measure all power modes (spec 0011).

## Open questions
- Roadmap row for the language switch (EN/VI setting + `vi` strings) not added yet.
- CI has no WDK build yet (T20); driver build is local only.
