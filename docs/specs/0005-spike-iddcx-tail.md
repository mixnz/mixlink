# 0005 — Spike S2b: cut the IddCx frame-path tail

**Status:** in-progress
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
- [ ] Each lever (L1–L4) has a Results row: applied or not (HRESULT), pooled A p99 vs baseline, lost.
- [ ] Final run (GPU default, 5 rounds): pooled IddCx p99 ≤ 1 ms **and** ≤ pooled DDA p99, pooled
  IddCx lost ≤ 0.1 %. Met → ADR-0006 holds. Not met → verdict is the DDA fallback.
- [ ] Every round with lost > 1 % has a stated cause from its burst report; a cause in the IddCx path
  counts as an IddCx failure, not a discarded round.
- [ ] Every summary carries the run conditions listed above.
- [ ] Spec 0004 Results corrected (round 3 conditions, A2 lost).
- [ ] Verdict written; if fallback, ADR-0006 and the T7/T21 rows' wording updated (roadmap change asked first).

## Open questions
- Does `SetGPUThreadPriority` / a HIGH D3D12 queue work from WUDFHost as LocalService, or does it need a
  privilege the host process lacks? Answered by the L2/L4 HRESULTs.
- If L3 sharing works, how long may the receiver hold the surface? Only matters if L3 is kept.
