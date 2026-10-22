# 0005 — Spike S2b: cut the IddCx frame-path tail

**Status:** done
**Date:** 2026-10-22
**Roadmap:** T23
**Plan:** [0005-spike-iddcx-tail](../plans/0005-spike-iddcx-tail.md) (local only, written by writing-plans skill)
**Crates touched:** none. Changed: `drivers/vdisplay/`, `tools/spikes/s2-iddcx-frames/`, spec 0004 Results.

## Goal
Find out whether the IddCx shared-texture path can meet spec 0004's latency criterion once its tail is
cut. If it can, ADR-0006 holds. If not, T7 and T21 switch to DDA on the virtual monitor (ADR-0006
fallback) with ADR-0005's fallback. The answer comes from repeated runs, not one good run.

## Scope
- In:
  - Driver levers L1–L4 (Design), each switchable at runtime, measured one at a time, then combined.
  - Harness: multi-round A/B runner, pooled stats, run conditions in every summary, display kept awake,
    lost-frame burst report.
  - Fix spec 0004 Results: round 3 (`222238`/`224412`) was the first "Prefer maximum performance" round,
    not "conditions unknown"; A2 (`235511`) lost 2 133 / 14 284 (14.9 %, 449 undecodable), not stated.
  - Results and Verdict in this spec; ADR-0006 updated only if the verdict is the fallback.
- Out:
  - `mixlink-capture`, production reader, service shell: T7, T19. Gate unchanged (spike is outside
    the workspace).
  - Topology, EDID, monitor add/remove, hybrid GPU tests: T21.
  - Encoding in the driver (would move the encoder into WUDFHost, against ADR-0005).
  - HDR, more than one monitor.

## Design

### Driver levers (all in `drivers/vdisplay/`, HOT PATH rules apply)
| ID | Lever | What changes | Expected effect |
|---|---|---|---|
| L1 | CPU priority | Frame thread MMCSS task `"Pro Audio"` + `AvSetMmThreadPriority(AVRT_PRIORITY_CRITICAL)` (now `"Distribution"`, normal) | Driver thread wakes on time when DWM hands over a frame |
| L2 | GPU priority | `IDXGIDevice::SetGPUThreadPriority(7)` on the driver device | Copy is scheduled ahead of other GPU work |
| L3 | No copy | Probe: `IDXGIResource1::CreateSharedHandle` on the acquired `pSurface`. If it succeeds, share the surface itself and drop the 14.7 MB copy | Removes the copy (avg part of ~310–340 µs) |
| L4 | D3D12 copy queue | IddCx 1.11 `IddCxSwapChainSetDevice2` with an `ID3D12Device`, `IddCxSwapChainReleaseAndAcquireBuffer2` with our own copy queue at `D3D12_COMMAND_QUEUE_PRIORITY_HIGH`; copy `pD3D12Surface` into the shared ring, signal a shared D3D12 fence | Copy on a dedicated copy engine, not behind 3D work |

- Facts checked in WDK `10.0.28000.2526` headers: `IDARG_IN_SWAPCHAINSETDEVICE` takes only an
  `IDXGIDevice*` (so D3D12 needs the 1.11 `...2` calls); `IDARG_IN_RELEASEANDACQUIREBUFFER2` has
  `pD3D12CommandQueue`; OS support is reported by `IDDCX_DEVICE_FEATURES_1_11_D3D12`.
- L4 needs the build to target IddCx 1.11 (`IDDCX_VERSION_MINOR=11`, minimum stays 4). If the OS lacks
  the D3D12 feature, the driver keeps the D3D11 path and records that; L4 is then reported as "not available".
- L2: if `SetGPUThreadPriority` fails (LocalService may lack the right), record the HRESULT and go on.
- L3 is a probe first: the surface belongs to the OS and may be reused after the next acquire, so even if
  sharing works, the receiver must finish with it before then. Results record the HRESULT and, if it works,
  whether frames arrive intact (counter cell matches).
- The receiver (`s2-svc`) gets the matching changes: MMCSS on its fence-wait thread (L1), GPU priority on
  its device (L2), D3D12 shared fence/textures if L4 needs them.
- **Lever switch without reinstalling the driver** (builder's call on details): the driver reads a lever
  bitmask from `HKLM\SOFTWARE\MixLink\S2b` (`Levers`, DWORD) when a swapchain is assigned and writes the
  active set (and any failure HRESULTs) into the shared header. The runner sets the value, then forces a new
  swapchain (mode 120 → 60 → 120, shown to work in spec 0004 A2). The header version goes to 2; layout
  stays mirrored in `src/header.rs`.

### Harness (`tools/spikes/s2-iddcx-frames/`)
- `run-case.ps1 -Levers <mask> -Rounds <n> -Minutes 2`: runs A, B, A, B, … for `n` rounds in one go,
  no one needs to watch it. Before the first round it checks: virtual monitor is the main display, main
  refresh 120 Hz, pattern is running; it stops with a clear message if not.
- Display kept awake for the whole run (`SetThreadExecutionState(ES_DISPLAY_REQUIRED | ES_CONTINUOUS)`
  in `s2-pattern`), so round 1's `ACCESS_LOST` storm cannot repeat.
- Every summary records conditions: lever mask requested and active, GPU power mode (`-GpuMode
  default|max` given by the user, plus `nvidia-smi` p-state and clocks sampled at start and end if
  present), main display and its refresh, OS build, driver build time.
- Pooled report: p50 / p99 / max and lost over all A rounds, and over all B rounds, plus a per-round
  table (p99, lost, undecodable) so the spread is visible.
- Lost-frame bursts: per round, list bursts (start QPC, length, undecodable count) so a > 1 % round can be
  explained (pattern app missed presents, display event, IddCx path).

### Measurement plan (user runs the scripts elevated; AI builds, reads logs, fills Results)
1. **Baseline**: current behaviour (mask 0), 3 rounds of 2 min A + 2 min B, GPU power mode **default**.
2. **Screening**: each lever alone (L1, L2, L3 if the probe succeeds, L4 if available), 3 rounds each.
   A lever that does not lower pooled A p99 against the baseline is dropped.
3. **Final**: the best combination of the kept levers, 5 rounds of 2 min A + 2 min B, GPU default.
4. **Diagnostic only**: the final combination once more with "Prefer maximum performance" (3 rounds).
   Not used for the verdict.

Same machine and setup as spec 0004 (RTX 5070, physical monitor at 120 Hz, virtual monitor as main).
Reference times as in spec 0004 (IddCx: driver acquire QPC; DDA: `LastPresentTime`).

## Latency impact
L1/L2/L4 change only scheduling of the existing copy and wake-up; L3 removes the copy. No CPU copy is
added on the frame path. Registry read happens once per swapchain, never per frame. Measured by the
harness above.

## Acceptance criteria
- [x] Each lever (L1–L4) has a Results row: applied or not (HRESULT), pooled A p99 vs baseline, lost.
  (L4: not available on IddCx 1.10.)
- [x] Final run (GPU default, 5 rounds): pooled IddCx p99 ≤ 1 ms **and** ≤ pooled DDA p99, pooled
  IddCx lost ≤ 0.1 %. Met → ADR-0006 holds. Not met → verdict is the DDA fallback.
  **Met:** 256 µs ≤ 1 000 µs and ≤ 422 µs; lost 0 %.
- [x] Every round with lost > 1 % has a stated cause from its burst report; a cause in the IddCx path
  counts as an IddCx failure, not a discarded round. (Only in the discarded first run: covered pattern.)
- [x] Every summary carries the run conditions listed above.
- [x] Spec 0004 Results corrected (round 3 conditions, A2 lost).
- [x] Verdict written; if fallback, ADR-0006 and the T7/T21 rows' wording updated (roadmap change asked first).
  (Not fallback: ADR-0006 and the roadmap rows unchanged.)

## Results
**Machine:** as spec 0004 (Windows 11 Pro 26H2 build 26300.9711, Secure Boot on, HVCI on, SAC off, one GPU
RTX 5070). OS IddCx framework `IddCx.dll` 10.0.26100.4202 = IddCx 1.10 (24H2), so **no D3D12 swapchains**.
Virtual monitor `\\.\DISPLAY6` = main display at 120 Hz, MSI not main. Every summary carries its `cond` lines
(levers, GPU mode, OS build, driver time, display check, nvidia-smi). Logs: `tools/spikes/s2-iddcx-frames/out/`.
Rounds of 2 min, A then B alternating, pooled by `s2-ctl pool`; p99 in µs, nearest rank.

| Run (GPU default unless noted) | IddCx p99 pooled (per round) | IddCx p50 | IddCx lost | DDA p99 pooled | Pool files |
|---|---|---|---|---|---|
| Baseline, mask 0 | 618 (552 / 614 / 700) | 244 | 101 / 43 112 (0.234 %) | 498 | `20261022-231226-L0-default-pool-*` |
| L1 CPU, mask 1 | **494** (489 / 498 / 496) | 243 | 0 % | 418 | `20261022-232431-L1-*` |
| L2 GPU, mask 2 | 614 (596 / 637 / 612) | 266 | 0 % | 422 | `20261022-233635-L2-*` |
| L3 no copy, mask 4 | **479** (392 / 596 / 263) | **85** | 0.002 % | 483 | `20261022-234840-L4-*` |
| L4 D3D12 | not available: needs IddCx 1.11 (`ERROR_NOT_SUPPORTED`) | | | | — |
| **Final, mask 7 (L1+L2+L3), 5 rounds** | **256** (262 / 249 / 261 / 250 / 260) | 68 | **0 / 71 891 (0 %)** | **422** | `20261023-000157-L7-default-pool-*` |
| Diagnostic, mask 7, "Prefer maximum performance" | 266 (263 / 275 / 261) | 79 | 0 % | 459 | `20261023-002326-L7-max-pool-*` |

Levers (from the summaries' `levers requested … active …` lines):
- **L1** applied in the driver and the receiver every time (no HRESULT). Evens out the rounds and halves the
  max (8.4 → 4.2 ms).
- **L2** applied (`SetGPUThreadPriority(7)` works from WUDFHost as LocalService), within noise of the
  baseline. Kept in the final mask by the screening rule (614 < 618 µs).
- **L3** applied: `CreateSharedHandle` on the IddCx surface works, the receiver reads it intact (0 undecodable).
  Removes the 14.7 MB copy: fence − acquire avg ~273 µs → ~80 µs.
- **L4** not measurable here (see Machine). Code kept, compiled only when building for IddCx ≥ 1.11.

Measurement notes:
- **Building for IddCx 1.11 breaks the driver on a 1.10 framework**: `IddCxAdapterInitAsync` returned
  `STATUS_INVALID_PARAMETER`, no monitor arrived (init steps logged to the event log, source `MixLinkVDisplay`).
  Building for 1.4 (as S2) fixed it. `IddCxGetVersion` returned `0x10C00` on both builds (not in Microsoft's table).
- **Baseline is already under 1 ms** (S2: 1.8–2.7 ms). Difference to S2: the virtual monitor is the main display
  at 120 Hz here (S2: MSI main, virtual secondary). The levers then cut p99 from 618 to 256 µs.
- **Covered pattern.** The first screening run lost L1-B round 3, L2 and L3 (1 frame per round or garbage
  counters, 11 410 duplicates): with the virtual monitor as main display, new windows opened over the pattern.
  `s2-pattern` is now always on top and `pool` marks rounds with > 1 % duplicates `INVALID`; that run was
  discarded and repeated (all rounds valid). Its baseline B round with 1.5 % lost had the same cause
  (218 consecutive undecodable frames at 65 s).
- No round of the repeated run lost > 1 %; baseline A round 1 lost 0.695 % (98 undecodable, one burst).
- Spec 0004 Results corrected (round 3 = first "Prefer maximum performance" round; A2 lost 14.9 %).

### Verdict: ADR-0006 holds
With L1 + L3 (+ L2) the IddCx shared-texture path meets spec 0004's criterion that S2 failed: pooled p99
**256 µs** against DDA's 422 µs, 0 frames lost over 71 891, steady across rounds (249–262 µs), and the same
with "Prefer maximum performance" (266 vs 459 µs). ADR-0006 and the T7/T21 rows stay as they are.

Constraints carried forward:
- **T7 (capture):** read the IddCx surface itself (L3), not a copy: the driver shares each surface once
  (per-surface NT handle, `SurfaceShare`) and the slot names it. The surface is only valid until the driver's
  next `IddCxSwapChainReleaseAndAcquireBuffer`, so the encoder must finish with it within that window (or the
  driver must hold the frame / fall back to the ring copy). T7 designs that hand-off.
- **T7 / T19:** frame thread in the driver and the service's frame-wait thread on MMCSS "Pro Audio" at critical
  priority (L1). `SetGPUThreadPriority(7)` (L2) is harmless but measured no gain; optional.
- **T21 / T20:** build `vdisplay` for the oldest IddCx it must run on (1.4 today) and use runtime checks for
  newer DDIs: a build for IddCx 1.11 fails adapter init on 24H2 (IddCx 1.10). L4 (D3D12 copy queue) stays
  untested until an OS with IddCx 1.11; with L3 there is no copy left for it to speed up.
- **T21:** virtual monitor as the main display at the client refresh (spec 0004) also gave the lower baseline.
- **Harness:** the pattern must stay on top once the virtual monitor is main; pool rejects covered rounds.

Clean-up (user, elevated): `uninstall-svc.ps1`, `tools\spikes\s1-signing\uninstall.ps1 -Case S2b-end`,
`reg delete HKLM\SOFTWARE\MixLink\S2b /f`, then make the MSI the main display again. Not run yet.

## Open questions
- (Answered) `SetGPUThreadPriority` works from WUDFHost as LocalService (L2 active, HRESULT 0). The HIGH D3D12
  queue could not be tried (IddCx 1.10 here).
- If L3 sharing works, how long may the receiver hold the surface? Only matters if L3 is kept.
  (L3 is kept: until the driver's next acquire. How the encoder fits in that window is T7's question.)
