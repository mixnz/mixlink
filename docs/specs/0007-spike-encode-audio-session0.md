# 0007 — Spike S4: hardware encode and WASAPI loopback from a Session 0 service

**Status:** in-progress
**Date:** 2026-10-23
**Roadmap:** T17
**Plan:** [0007-spike-encode-audio-session0](../plans/0007-spike-encode-audio-session0.md) (local only, written by writing-plans skill)
**Crates touched:** none. New: `tools/spikes/s4-encode-audio/`

## Goal
Prove or disprove the risk ADR-0005 names: hardware video encoders (NVENC, QSV; AMF untested) and
WASAPI loopback work from a LocalSystem service in Session 0, **and are not slower there** than in a user
session. T19, T7 and T11 wait on this answer. Fallback if it fails: the pipeline runs in a SYSTEM
process started in the console session (ADR-0005).

## Scope
- In:
  - `tools/spikes/s4-encode-audio/`: standalone Cargo package (own `[workspace]`, throwaway), binaries:
    - `s4-svc`: service `MixLinkS4` (LocalSystem, Session 0, demand start). Runs one job per start
      (`encode` or `audio`, args from the service config), writes logs to `out/`, stops itself.
      The same binary runs the same job with `--console` in the user session (comparison runs).
    - `s4-tone`: plays a test tone with timing marks on the default output (user session).
    - `s4-ctl`: `pool` (pooled stats over rounds, like `s2-ctl pool`), `check` (lists adapters,
      encoders FFmpeg finds, audio endpoints).
  - Scripts (user runs them elevated): `install-svc.ps1`, `uninstall-svc.ps1`, `run-case.ps1`
    (alternating Session 0 / user-session rounds, like S2b), `report.ps1` (versions, GPU, driver, power plan).
  - Machines: main PC (RTX 5070) for NVENC and audio; Intel laptop (iGPU) for QSV and audio. No AMD GPU:
    AMF is recorded as **untested**.
  - Results written into this spec in S1–S3's format, and the verdict for ADR-0005.
- Out:
  - Capture from IddCx or DDA (frames are drawn by the spike itself; T7 wires real capture).
  - Opus encode, network, playback on a client (T11). Decode on the client (T8).
  - Production `mixlink-codec` / `mixlink-audio` API, `apps/svc` shell (T19).
  - AMF (no hardware), AV1, direct NVENC SDK unless FFmpeg itself fails in Session 0 (Design → Fallback probe).
  - Shipping FFmpeg: the user puts an LGPL shared FFmpeg build in place by hand; scripts only check it.

## Design

### Encode job (`s4-svc encode --codec h264|hevc --backend nvenc|qsv`)
- D3D11 device on the chosen adapter (by vendor id; NVIDIA for NVENC, Intel for QSV), no UI, no swapchain.
- FFmpeg via `ffmpeg-next` (ADR-0001), D3D11 hw device + hw frames context (`AV_PIX_FMT_D3D11`, NV12
  or BGRA as the encoder accepts; chosen and verified in the plan). Pool of preallocated hw frames.
  QSV: QSV device derived from the D3D11 device. **No CPU copy:** no `av_hwframe_transfer_data`,
  no staging texture anywhere in the encode path.
- Encoder settings (low latency): no B-frames, 1-frame VBV, intra-refresh, CBR 30 Mbit/s, `tune=ull` /
  `zerolatency` (NVENC), `async_depth=1` + low-power mode (QSV). Exact option names verified in the plan.
- Frame source: a compute or draw pass writes a pattern into the next pool texture, with the frame
  number encoded in a block grid (decodable from the bitstream). Each frame: draw, wait for the GPU
  (event query), then QPC `t0`, `send_frame`, loop `receive_packet` until the packet for this frame,
  QPC `t1`. Encode latency = `t1 − t0`. Paced at 120 fps (waitable timer, high resolution).
- Default size 2560x1440@120 (same as S2). Laptop: same size if the iGPU keeps up, else 1920x1080@60 (recorded).
- Per run: N = 10 000 frames (~83 s). Arrays preallocated; stats after the run (p50 / p99 / max, frames
  out vs in). First 600 packets also go to `out/<stamp>.h264|.hevc` for the validity check.
- Validity check: `ffprobe -count_frames` on that file = 600 frames, 0 decode errors; `s4-ctl` decodes
  frames 0, 299, 599 and reads the frame number back from the block grid.

### Audio job (`s4-svc audio --minutes M [--tone]`)
- WASAPI shared mode, `AUDCLNT_STREAMFLAGS_LOOPBACK | EVENTCALLBACK` on the default render endpoint,
  MMCSS "Pro Audio" thread. Requests the smallest engine period the endpoint allows (`IAudioClient3`);
  records the period it got.
- Per packet: frames, flags (silent, discontinuity, timestamp error), device QPC position from
  `GetBuffer`. Latency = QPC now − device QPC position. Interval = time between packets.
- Device change: `IMMNotificationClient` on default render change; the job reopens the stream and logs
  the gap (time from change to first packet on the new device).
- Signal check: RMS per second; "non-silent" = above −50 dBFS while a tone plays.
- `--tone`: the job also plays its own tone from Session 0 (render stream, same process). Used where no
  user session can play one (login screen, lock screen).
- `s4-tone`: sine 1 kHz in the user session; the main case of real interest (user audio → Session 0).

### Comparison and pooling
- Same binary, same args: A = service in Session 0, B = `--console` in the user session (normal user,
  not elevated). `run-case.ps1 -Rounds 3` runs A, B, A, B, A, B; `s4-ctl pool` pools per side.
- "Not slower" (my call, user agreed on ~10 %): pooled p99 A ≤ max(1.1 × p99 B, p99 B + 0.2 ms).
  The 0.2 ms floor keeps noise from failing a sub-2 ms result.
- GPU power mode at its default; `report.ps1` records it. Pattern app or game: none running.

### Fallback probe
- If A fails (cannot open the encoder or audio client) or is slower than allowed: run the job as SYSTEM in
  the console session (`s4-svc` spawns itself like S2's `console_spawn`) = case C, so the verdict can name
  the fallback with numbers.
- If FFmpeg fails in Session 0 but works in B: one direct NVENC SDK open-session call (`nvEncOpenEncodeSessionEx`
  on the D3D11 device) separates "FFmpeg" from "Session 0". Only then; not planned work.

### Test cases
| Case | Machine | What | Expected if ADR-0005 holds |
|---|---|---|---|
| A. Smoke | main | install service, `s4-ctl check`, 1 short encode + audio job | service runs; NVIDIA adapter, `h264_nvenc` / `hevc_nvenc` listed; endpoints listed |
| B. NVENC H.264 | main | `run-case.ps1 -Job encode -Codec h264 -Rounds 3` | A p99 ≤ 3 ms, 0 lost; A not slower than B |
| C. NVENC HEVC | main | same, `-Codec hevc` | same as B |
| D. Valid bitstream | main | validity check on B and C output | 600 frames, 0 errors, frame numbers 0 / 299 / 599 read back |
| E. Session states | main | 1-min encode + `--tone` audio job: at login screen after reboot, on lock screen, with UAC prompt open | both jobs run, numbers like B; audio non-silent |
| F. QSV | laptop | `-Backend qsv -Codec h264 -Rounds 3` (HEVC if listed) | runs in A; p99 recorded (no target); A not slower than B |
| G. Loopback, user audio | main | `s4-tone` in the user session, `run-case.ps1 -Job audio -Minutes 10 -Rounds 1` | non-silent; packet interval ≤ 5 ms; 0 discontinuities; latency recorded |
| H. Loopback, no user | main | `--tone` job at login screen and on lock screen, 2 min each | runs, non-silent (own tone), 0 discontinuities |
| I. Device change | main | switch default output during G (needs 2 outputs, e.g. speakers + monitor HDMI) | reopens on the new device, gap ≤ 1 s, logged |
| J. Laptop audio | laptop | G, 2 min, 1 round | same as G |
| K. Fallback | as needed | only if A fails or is slower: case C runs | recorded for the verdict |

Scripts install a service, so **the user runs them** (elevated); the AI session builds, reads logs and fills
Results. Logs: `tools/spikes/s4-encode-audio/out/` (gitignored). Laptop: the user copies the release build
plus FFmpeg DLLs (a `pack.ps1` zip from the main PC; Rust is not needed there).

### Machines
Main: Win11 26H2, RTX 5070, same as S1–S3; NVIDIA driver version recorded. ViGEmBus / S2b leftovers do not
matter here. Laptop: Intel iGPU model, Windows build and graphics driver recorded by `report.ps1`.

## Latency impact
Measures two host stages from Session 0: encode (budget ≤ 3 ms, `latency-budget.md`) and audio loopback
(engine period / capture latency, feeding 5 ms Opus frames). No production hot path changes.

## Acceptance criteria
- [ ] A: the service lists the NVIDIA adapter, NVENC encoders and audio endpoints from Session 0.
- [ ] B, C: NVENC H.264 and HEVC from Session 0: p99 ≤ 3 ms, 0 lost, and not slower than the user
  session (rule in Design → Comparison).
- [ ] D: Session 0 output decodes (600 frames, 0 errors, frame numbers read back); no CPU copy in the path.
- [ ] E: encode and loopback keep working at the login screen, on the lock screen and with UAC open.
- [ ] F: QSV H.264 encodes from Session 0 on the laptop; p99 recorded, not slower than the user session.
- [ ] G–J: loopback of user-session audio from Session 0: non-silent, 0 discontinuities, packet interval
  ≤ 5 ms (or the smallest the endpoint allows, recorded), device change recovered within 1 s.
- [ ] AMF recorded as untested (no hardware).
- [ ] Results filled (cases table with Expected / Observed / Log) and the verdict: ADR-0005 holds, holds
  with changes (named), or the pipeline moves to a SYSTEM process in the console session.

## Results
(Filled after the runs.)

## Open questions
- Shared-mode loopback usually follows the render engine period (often 10 ms). If 5 ms is not reachable,
  T11 decides: 10 ms capture cut into two 5 ms Opus frames (+ up to 5 ms), or exclusive / `IAudioClient3`
  low-latency render on the host. The spike records what each machine gives.
- FFmpeg build: which LGPL shared build (version) and whether `ffmpeg-next` needs libclang to build;
  settled in the plan, before any code.
- AMF on Session 0 stays unproven until an AMD machine is available (phase 2 test matrix).
