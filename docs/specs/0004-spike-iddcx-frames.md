# 0004 — Spike S2: IddCx frames into a shared texture for the service

**Status:** done
**Date:** 2026-10-22
**Roadmap:** T15
**Plan:** [0004-spike-iddcx-frames](../plans/0004-spike-iddcx-frames.md) (local only, written by writing-plans skill)
**Crates touched:** none. Changed: `drivers/vdisplay/`. New: `tools/spikes/s2-iddcx-frames/`

## Goal
Prove or disprove ADR-0006's target frame path: the IddCx driver copies each swapchain frame into a
shared D3D11 texture and signals a fence, and a LocalSystem service in Session 0 (ADR-0005) opens it
with no CPU copy, at least as fast as DXGI Desktop Duplication (DDA) on the same virtual monitor.
T7 and T21 wait on this answer.

## Scope
- In:
  - `drivers/vdisplay/` (kept, built on S1):
    - one virtual monitor (sample creates 3) with mode **2560x1440@120** added to its mode list;
    - frame sharing: ring of shared textures + shared fence + a small shared-memory header (Design);
    - render adapter: on a machine with iGPU + dGPU, call `IddCxAdapterSetRenderAdapter` with the dGPU.
  - `tools/spikes/s2-iddcx-frames/`: one standalone Cargo package (own `[workspace]`, not a member of
    the main workspace, throwaway), three binaries:
    - `s2-pattern`: test pattern app in the user session (Design);
    - `s2-svc`: the receiver, a real Windows service (LocalSystem, Session 0, installed with `sc create`);
    - `s2-dda`: DDA probe, started by `s2-svc` in the console session as SYSTEM.
  - Scripts (user runs them elevated): `install-svc.ps1` / `uninstall-svc.ps1` (`sc create` / `sc delete`,
    auto-start), `run-case.ps1 <case>` (sets the mode, starts a run, collects logs into `out/`).
    The driver itself is signed and installed with S1's `tools/spikes/s1-signing/install.ps1`.
  - Results written into this spec in S1's format, and the verdict for ADR-0006 (holds / DDA fallback).
- Out:
  - `mixlink-capture` API (`FrameSource`) and the production reader: T7.
  - IOCTL control, per-client EDID and modes, topology switching, monitor add/remove at runtime: T21.
  - HDR, more than one monitor, cursor channel, encode.
  - Production service shell (`apps/svc`): T19. Gate is unchanged (spike package is outside the workspace).

## Design

### Driver side (WUDFHost, LocalService, Session 0)
- On swapchain assign, on the render adapter's device: create **N = 4** textures (2560x1440,
  `B8G8R8A8_UNORM`, `D3D11_RESOURCE_MISC_SHARED_NTHANDLE`) and one fence (`D3D11_FENCE_FLAG_SHARED`),
  plus NT handles for each (`CreateSharedHandle`).
- Shared memory section `Global\MixLinkS2Frames`, DACL: SYSTEM + LocalService only. Header: WUDFHost PID,
  handle values (textures, fence), adapter LUID, width/height/format, and per slot: sequence number,
  `PresentationFrameNumber`, `PresentDisplayQPCTime`, driver acquire QPC.
- Per frame (HOT PATH, no allocation): `ReleaseAndAcquireBuffer` → read the acquire QPC →
  `CopyResource` from `pSurface` into slot `seq % N` → write the slot's header → `Signal(fence, seq)` →
  `Flush` → `FinishedProcessingFrame`. One GPU copy, no CPU access to pixels.
- Handle transfer: the service opens WUDFHost (`PROCESS_DUP_HANDLE`) and calls `DuplicateHandle`.
  If that is refused, fallback: named shared handles opened by name (record which one worked).

### Receiver (`s2-svc`, LocalSystem, Session 0)
- Waits for the section, duplicates the handles, creates a D3D11 device **on the LUID from the header**,
  opens the textures (`OpenSharedResource1`) and the fence (`OpenSharedFence`).
- Loop: `SetEventOnCompletion(next_seq)` → wait → QPC = "fence signalled" time → read the slot header.
  Results go into preallocated arrays (≥ 80 000 entries per run); stats and CSV are written after the run.
- Frame number check (harness only): `CopySubresourceRegion` of the pattern's counter cell into a tiny
  staging texture → `Map` → decode. This is the only CPU readback in timing runs.
- PNG mode (evidence cases only, never during timing runs): copy the full slot into a staging texture,
  `Map`, write PNG to `out/`, every 5 s.
- Records which GPUs exist (DXGI adapter list with LUIDs) and the LUID the frames are on.

### DDA probe (`s2-dda`)
DDA does not work from Session 0, so `s2-svc` starts `s2-dda` in the active console session with its own
token (`WTSGetActiveConsoleSessionId`, `DuplicateTokenEx`, `SetTokenInformation(TokenSessionId)`,
`CreateProcessAsUser`): the same layout as ADR-0005's fallback. It duplicates the virtual monitor's output
(`DuplicateOutput1`), and per frame takes QPC when `AcquireNextFrame` returns, reads `LastPresentTime`
and the counter cell, then `ReleaseFrame`. Same arrays, stats and CSV as the receiver.

### Test pattern (`s2-pattern`, user session)
Borderless full-screen window on the virtual monitor, D3D11 flip-model swapchain, vsync on. Every frame:
new background color, frame counter drawn as 32 black/white blocks (8x8 px each) in a fixed cell at the
top-left. It logs its own missed presents (`GetFrameStatistics`), so drops caused by the app itself are
not counted against the frame path.

### Measurement (same reference on both paths: DWM present time, QPC)
- IddCx latency = fence-signalled QPC (receiver) − `PresentDisplayQPCTime`. If `PresentDisplayQPCTime`
  is later than the driver acquire QPC, the driver acquire QPC is the reference instead; Results states
  which reference was used and for how many frames.
- DDA latency = `AcquireNextFrame` return QPC − `LastPresentTime`.
- Also recorded (diagnostic only): driver acquire QPC − `PresentDisplayQPCTime`, and fence − acquire.
- Lost frame = a gap in the counter seen by the receiver (or a slot whose counter does not match its
  header, i.e. overwritten), minus gaps the pattern app logged as its own missed presents.
- Run length 10 min at 120 fps (~72 000 frames); report p50, p99, max and lost count / %.
- QPC is one system-wide clock, so values from WUDFHost, Session 0 and the console session compare directly.

### Test cases
| Case | What | Expected if ADR-0006 holds |
|---|---|---|
| A. IddCx timing | pattern on the virtual monitor, `s2-svc` 10 min | p99 ≤ 1 ms; lost ≤ 0.1 % |
| B. DDA timing | same, `s2-dda` 10 min (separate run, not at the same time as A) | p99(A) ≤ p99(B) |
| C. PNG desktop | PNG mode, normal desktop | PNG shows the desktop with the pattern |
| D. PNG UAC | PNG mode, a UAC prompt is open | PNG shows the UAC prompt |
| E. PNG lock | PNG mode, Win+L | PNG shows the lock screen |
| F. PNG login after reboot | reboot, do not log in | service and virtual monitor up from boot; PNG shows the login screen |
| G. Adapter | adapter list + frame LUID from A | hybrid: frames on the dGPU; single GPU: noted in Results |
| H. No CPU copy | code check of driver + receiver frame path | only GPU copies; the counter cell is the only `Map` in timing runs |

Scripts change services and drivers on the machine, so **the user runs them**; the AI session builds,
reads logs and fills Results. Logs: `tools/spikes/s2-iddcx-frames/out/` (gitignored).

## Latency impact
Adds one GPU copy (2560x1440 BGRA, ~14.7 MB) per frame in the driver and one fence signal; this is what the
spike measures. No CPU copy on the frame path.

## Acceptance criteria
- [ ] A: IddCx latency p99 ≤ 1 ms, and p99(A) ≤ p99(B). **Not met** (p99 1 776–2 670 µs; DDA 881–1 903 µs) → T23.
- [x] A: lost frames ≤ 0.1 % over 10 min at 120 fps (A 0 %, AP 0.099 %).
- [x] H: no CPU copy on the frame path (counter cell readback is harness only).
- [x] C–F: PNGs saved by `s2-svc` for desktop, UAC, lock screen and login screen after reboot (not logged in),
  with the virtual monitor present from boot (lock/login need the virtual monitor as main display: E2, F).
- [x] G: on iGPU + dGPU, frames are on the dGPU (LUID recorded); with one GPU, Results says so.
- [x] Results section filled (cases table with Expected / Observed / Log, as in spec 0003) and the verdict:
  ADR-0006 holds, or switch to DDA on the virtual monitor (ADR-0006 fallback) with ADR-0005's fallback.

## Results
**Machine:** Windows 11 Pro 26H2 build 26300.9711, Secure Boot on, HVCI on, SAC Evaluation, test-signing off.
One GPU: NVIDIA GeForce RTX 5070 (12 GB). Physical monitor MSI MPG 274U (160 Hz, set to **120 Hz** for timing
runs). Driver signed and installed with S1's `install.ps1` (pnputil 0, problem code 0, one extra monitor).
Logs: `tools/spikes/s2-iddcx-frames/out/` (local only; PNGs show the account name, never copy them into docs).

| Case | Expected | Observed | Log |
|---|---|---|---|
| A (run 2, GPU default power) | p99 ≤ 1 ms; lost ≤ 0.1 % | **Fail.** p50 242 µs, p99 **2 670 µs**, max 10 085 µs; lost 0 / 71 978; one generation | `20261021-215628-A-iddcx` |
| B (run 2) | p99(A) ≤ p99(B) | DDA p50 106 µs, p99 1 903 µs, max 14 422 µs; lost 41 / 72 001 (0.057 %) → **p99(A) > p99(B)** | `20261021-220642-B-dda` |
| AP (GPU "Prefer maximum performance") | as A | **Fail.** p50 242 µs, p99 **1 776 µs**, max 16 147 µs; lost 71 / 71 982 (**0.099 %**, 1–5 frames at a time) | `20261021-230618-AP-iddcx` |
| BP (same GPU mode) | p99(AP) ≤ p99(BP) | DDA p50 150 µs, p99 881 µs, max 16 453 µs; lost 102 / 72 000 (0.142 %) → **p99(AP) ≈ 2 × p99(BP)** | `20261021-231656-BP-dda` |
| A2 (120 → 60 → 120 Hz while measuring) | receiver survives new swapchains | **Pass.** Generations 3 → 4 (60 frames/s) → 5 (120 frames/s), reopened by itself each time, p99 1 559 µs; lost 2 133 / 14 284 (14.9 %, 449 undecodable), cause not found (corrected by spec 0005) | `20261021-235511-A2-iddcx` |
| C (desktop) | PNG of the desktop | **Pass.** 12 PNGs, pattern with counter cell | `20261021-234348-C` … |
| D (UAC) | PNG of the UAC prompt | **Pass.** Dimmed secure desktop for 80 s (dialog on the main display), then desktop | `20261021-235729-D` … |
| E (Win+L, virtual monitor not main) | PNG of the lock screen | **Partial.** Virtual monitor all black while locked (Windows draws the lock screen on the main display); new swapchain on unlock, reopened | `20261022-000058-E` … |
| E2 (Win+L, virtual monitor main) | as E | **Pass.** Lock screen (clock, "Enter your PIN"), sign-in screen incl. a wrong-PIN dialog, then desktop | `20261022-000707-E2` … |
| F (reboot, not logged in, virtual monitor main) | monitor from boot; PNG of the login screen | **Pass.** Boot 00:13:12 UTC; service waited 1 s for the section, frames from 00:13:23; lock screen from 00:13:29, PIN screen 00:13:49–14:14, then desktop | `20261022-001322-F` … |
| G (adapter) | dGPU, or "one GPU" noted | **One GPU.** Frames on the RTX 5070 (LUID `0000C311`, `0000C8D0` after reboot: LUIDs change per boot). Before the reboot DXGI listed the RTX 5070 **twice** (second LUID `03294BCA`); after it, once | all summaries |
| H (no CPU copy) | only GPU copies | **Pass.** `FrameShare::Publish`: one `CopyResource` GPU→GPU + fence signal; `receiver.rs`: the only `Map` is the 256x8 counter cell (timing) or the PNG (png mode only) | code |

Measurement notes:
- **Reference time.** `PresentDisplayQPCTime` was later than the driver acquire on every frame of A and AP
  (0.9–12 ms, avg 4–8 ms; A2: all but 2 frames), so IddCx latencies use the **acquire** QPC (rule from Open questions). DDA uses DWM's
  `LastPresentTime`. The two start points differ: DDA's includes DWM's own present-to-copy time, IddCx's
  starts after DWM has handed the frame to the driver.
- **Tail is not the harness.** Frames > 1 ms that arrived < 100 µs after the previous counter-cell `Map`:
  A 9 / 2 531, B 0 / 1 757; dropping them leaves p99 unchanged. The tail is spread evenly over the 10 min.
- **GPU clocks matter.** "Prefer maximum performance" cut the counter-cell `Map` p99 from ~2 ms to 0.7 ms and
  both p99s, but the IddCx tail stayed about twice DDA's. IddCx latency = driver `CopyResource` (14.7 MB) +
  fence signal + service wake-up: avg ~310–340 µs.
- **Refresh follows the main display.** With the MSI at 160 Hz, DWM composed the 120 Hz virtual monitor at
  **160 fps** (DDA 9 601 frames / 60 s, 0 lost). ADR-0006's "client refresh, no judder" needs the virtual
  monitor to drive composition (main display / virtual-only topology).
- **Lock and login show only on the main display**: the virtual monitor must be main (ADR-0006 topology 1
  or 2) for E/F to stream, as E vs E2 shows.
- Discarded runs: run 1 (primary screen slept; DDA spun on `ACCESS_LOST` 335 233 times, harness fixed in
  `ab24ce8`), run 3 at 05:12–05:44 local (A lost 11 %, B 0.115 %). Corrected by spec 0005: run 3 was the
  first "Prefer maximum performance" round (user's account), not "conditions unknown"; AP/BP repeated it.
- **DDA needs per-monitor DPI awareness**: `DuplicateOutput1` returns `DXGI_ERROR_UNSUPPORTED` otherwise.
- Driver: `PickRenderAdapter` first counted DXGI entries, so the duplicate RTX 5070 entry made this
  single-GPU machine look hybrid. Fixed after the runs: it now counts physical GPUs (vendor, device,
  subsystem, revision). Built, not re-run on hardware (no hybrid machine; driver already uninstalled).

### Verdict: latency criteria not met; ADR-0006 stays, pending T23
The IddCx shared-texture path works end to end from a LocalSystem service in Session 0: no CPU copy, ≤ 0.1 %
lost, survives new swapchains, and carries the UAC secure desktop, the lock screen and the login screen after
reboot (C, D, E2, F, A2, H). It fails criterion 1: p99 1.8–2.7 ms, above 1 ms and above DDA's p99.
Decision (user, 2026-10-22): **ADR-0006 is not changed yet.** T23 tries to cut the IddCx tail (driver copy
thread / GPU priority, removing the 14.7 MB copy) and measures again with the same harness. If T23 still
fails criterion 1, switch to DDA on the virtual monitor (ADR-0006 fallback) with ADR-0005's fallback.

Constraints carried forward (both frame paths):
- Virtual monitor must be the **main display** while streaming: composition rate and lock/login screens
  follow the main display (ADR-0006 topology 1 or 2). T21.
- Hybrid detection by physical GPU, not by DXGI entry count (done in `PickRenderAdapter`; T21 tests it on
  an iGPU + dGPU machine). LUIDs change per boot: never persist one. T21.
- A DDA reader must be per-monitor DPI aware and back off on `DXGI_ERROR_ACCESS_LOST`. T7 (if fallback).

## Open questions
- (Answered 2026-10-22) `PresentDisplayQPCTime` is documented as the time the surface "should be
  displayed" and may be later than DWM's present. Rule: when it is later than the driver acquire QPC,
  use the acquire QPC as the IddCx reference and say so in Results (see Measurement).
