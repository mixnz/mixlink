# 0004 — Spike S2: IddCx frames into a shared texture for the service

**Status:** in-progress
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
- [ ] A: IddCx latency p99 ≤ 1 ms, and p99(A) ≤ p99(B).
- [ ] A: lost frames ≤ 0.1 % over 10 min at 120 fps.
- [ ] H: no CPU copy on the frame path (counter cell readback is harness only).
- [ ] C–F: PNGs saved by `s2-svc` for desktop, UAC, lock screen and login screen after reboot (not logged in),
  with the virtual monitor present from boot.
- [ ] G: on iGPU + dGPU, frames are on the dGPU (LUID recorded); with one GPU, Results says so.
- [ ] Results section filled (cases table with Expected / Observed / Log, as in spec 0003) and the verdict:
  ADR-0006 holds, or switch to DDA on the virtual monitor (ADR-0006 fallback) with ADR-0005's fallback.

## Results
(filled after the runs)

## Open questions
- (Answered 2026-10-22) `PresentDisplayQPCTime` is documented as the time the surface "should be
  displayed" and may be later than DWM's present. Rule: when it is later than the driver acquire QPC,
  use the acquire QPC as the IddCx reference and say so in Results (see Measurement).
