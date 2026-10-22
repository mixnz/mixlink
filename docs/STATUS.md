# Status

> Replace content each update; keep ≤ 80 lines. History lives in git, not here.

**Updated:** 2026-10-22
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
- T15 spike S2 (spec 0004): **latency criteria not met, ADR-0006 kept pending T23.**
  - `drivers/vdisplay`: one virtual monitor 2560x1440@120, `FrameShare` (4 shared textures + fence +
    `Global\MixLinkS2Frames` header), render-adapter pick.
  - `tools/spikes/s2-iddcx-frames/`: pattern app, receiver service `MixLinkS2`, DDA probe, scripts.
  - Service in Session 0 gets every frame (≤ 0.1 % lost, no CPU copy), UAC/lock/login screens, survives
    new swapchains. IddCx p99 1.8–2.7 ms vs DDA 0.9–1.9 ms.
  - Virtual monitor must be the main display (composition rate and lock/login follow it).

## In progress
- Nothing. Test machine is clean (S2 driver and service removed).

## Next
- Manual checks left from T1: host tray Open/X/Quit, stream resize with debug layer, `npm run dev:host`.
- Roadmap phase 1: T23 (cut the IddCx tail, re-measure; fails → DDA fallback), T16 spike S3 (UMDF HID).
  T20 follows spec 0003 → Verdict; T21 follows spec 0004 → Verdict constraints.

## Open questions
- Roadmap row for the language switch (EN/VI setting + `vi` strings) not added yet.
- CI has no WDK build yet (T20); driver build is local only.
