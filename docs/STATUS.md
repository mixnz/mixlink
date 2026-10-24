# Status

> Replace content each update; keep ≤ 80 lines. History lives in git, not here.

**Updated:** 2026-10-23
**Phase:** 1 — LAN MVP

## Done
- Repo structure, AI working rules, architecture docs; ADR-0001/0002/0003 accepted.
- Quality gate (`scripts/gate.sh`): text, lines, links, docs, crate deps, UI (npm), fmt, clippy, tests.
- CI: `ci.yml` (docs/lint/test, Node from `.nvmrc`) on dispatch via `scripts/ask-ci.sh`; `/execute`, `/land`, `/watch-ci`.
- Cargo workspace (spec 0001): toolchain 1.99.1, lib crates + apps, workspace lints, profiles.
- T1 app shells (spec 0002):
  - `mixlink-core::ipc`: launcher <-> stream JSON-lines contract.
  - `mixlink-render::Presenter`: D3D11 flip-model swapchain (WARP fallback, debug layer in debug).
  - `apps/stream`: winit window, solid color, `Ready`/`Closed` over stdout, closes on stdin EOF.
  - `apps/client`: Tauri launcher, spawns/supervises the stream, closes its webview while streaming.
  - `apps/host`: Tauri tray shell (Open / Quit, close hides).
  - npm workspace: `@mixlink/i18n` + both React UIs; `bash scripts/build.sh release` → 3 exes in `dist/`.
- Host architecture ADRs accepted: 0004 UMDF-only drivers signed per machine, 0005 host pipeline in
  `mixlink-svc` service, 0006 always stream from IddCx virtual display, 0007 input via virtual HID.
- Skill `natural-copy` for user-facing text; UI languages EN (default) + VI (ADR-0003, `en` only so far).
- T14 spike S1 (spec 0003): **ADR-0004 holds.**
  - `drivers/vdisplay`: IddSampleDriver renamed; `drivers/build.ps1` builds it from NuGet WDK
    `10.0.28000.2526` (needs VS Build Tools components: C++, Spectre libs, Windows Driver Kit).
  - `tools/spikes/s1-signing/`: install/uninstall/report scripts (user runs them elevated).
  - Win11 26H2, Secure Boot on: per-machine signed catalog installs, loads, survives reboot, HVCI on/off;
    untrusted signer refused; tampered DLL flagged at install but not re-checked at load.
- T15 spike S2 (spec 0004): IddCx frames reach a Session 0 service (≤ 0.1 % lost, no CPU copy, UAC/lock/
  login screens, new swapchains); p99 1.8–2.7 ms failed the latency criterion → T23.
  - `drivers/vdisplay`: one virtual monitor 2560x1440@120, `FrameShare` ring + fence + shared header.
  - `tools/spikes/s2-iddcx-frames/`: pattern app, receiver service `MixLinkS2`, DDA probe, scripts.
- T23 spike S2b (spec 0005): **ADR-0006 holds.** With L1 (MMCSS "Pro Audio" critical) + L3 (share the IddCx
  surface itself, no 14.7 MB copy): IddCx p99 256 µs vs DDA 422 µs, 0 lost over 5 rounds.
  - Driver levers switchable at runtime (registry), header v2; harness runs A/B rounds unattended and pools
    them (`run-case.ps1 -Levers -Rounds`, `s2-ctl`), pattern always on top, covered rounds flagged.
  - Build `vdisplay` for IddCx 1.4: a 1.11 build breaks adapter init on 24H2 (IddCx 1.10). L4 (D3D12) untested.
  - Virtual monitor must be the main display (composition rate, lock/login, and the lower baseline).
- T16 spike S3 (spec 0006): **ADR-0007 holds, with changes.**
  - `drivers/vhid`: vhidmini2 UMDF HID minidriver, keyboard + 2 mice + gamepad + vendor control collection;
    `build.ps1 -Driver vdisplay|vhid|all`. Not flagged injected; reaches UAC and lock screen from Session 0.
  - `tools/spikes/s3-vhid/`: install/report scripts + Rust harness (`s3-latency`, `s3-feed`, `s3-apis`,
    `s3-hook`, SYSTEM runner). Measure elevated (UIPI hides input from a non-elevated receiver).
  - Games: kbd/mouse direct in all 3; pad via Steam Input (manual layout; analog LT/RT ignored); ViGEm for
    Steam-Input-off XInput (Wuchang). p99: kbd 0.80, abs mouse 0.62, rel mouse ~3.3 (SendInput ~3.0), pad
    Raw Input ~1.1, WGI 0.54, ViGEm 0.06 ms.
- T17 spike S4 (spec 0007): **ADR-0005 holds, with changes** (ADR amended: encode priority).
  - `tools/spikes/s4-encode-audio/`: FFmpeg 8.1 (LGPL, `C:\tools\ffmpeg`, LLVM for bindgen) NVENC/QSV encode
    job and WASAPI loopback job, as service `MixLinkS4` or in the user session; `run-all.ps1` per machine.
  - NVENC 1440p120 in Session 0: p99 5.1 vs 3.1 ms (user) at default priority; with MMCSS + GPU priority 7 +
    high process priority 3.15 vs 2.95 ms. Console-session SYSTEM (fallback) behaves the same. Works at login
    (after reboot), lock, UAC; bitstreams valid, no CPU copy. QSV (laptop iGPU) works via decoder-bind pool.
  - Loopback: 10 ms packets (`IAudioClient3` refuses loopback), nothing while the endpoint is idle.
- T18 spike S5 (spec 0008): **launch risk passes** for EA Javelin (Apex Legends) and EAC (Elden Ring), with
  our drivers installed, virtual monitor main, and after removal (8/8). Vanguard, BattlEye, FACEIT, gameplay
  and input untested: user warning in `docs/compat/anti-cheat.md`. `tools/spikes/s5-anticheat/run-all.ps1`.

## In progress
- Nothing. Main PC and laptop: `MixLinkS4` removed; main PC: S5 drivers removed (`RESULT CLEAN`). Test
  machine: ViGEmBus may still be installed (Settings → Apps). S2b clean-up (spec 0005 → Verdict) not confirmed.

## Next
- Manual checks left from T1: host tray Open/X/Quit, stream resize with debug layer, `npm run dev:host`.
- Roadmap phase 1: T2 next. T20 follows specs 0003 and 0008 → Verdict (`vhid` held open by Steam blocks
  uninstall); T21 follows spec 0004 and 0005 → Verdict constraints, 0008 (main display via `SetDisplayConfig`); T7 follows specs 0005 and 0007 → Verdict (encode levers, isolate them; QSV
  pool); T11 follows spec 0007 (10 ms loopback, silence fill); T22 / T12 follow spec 0006 → Verdict.

## Open questions
- Roadmap row for the language switch (EN/VI setting + `vi` strings) not added yet.
- CI has no WDK build yet (T20); driver build is local only.
