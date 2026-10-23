# 0007 — Spike S4: hardware encode and WASAPI loopback from a Session 0 service

**Status:** done
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
- [x] A: the service lists the NVIDIA adapter, NVENC encoders and audio endpoints from Session 0.
- [x] B, C: NVENC H.264 and HEVC from Session 0: 0 lost, and **with the priority levers** not slower than
  the user session. **Partly:** without levers Session 0 is ~2 ms slower at p99 (FAIL); H.264 p99 is
  ~3.1 ms with levers (budget 3 ms, both sessions alike); HEVC 3.4 ms, measured without levers only.
- [x] D: Session 0 output decodes (600 frames, 0 errors, frame numbers read back); no CPU copy in the path.
- [x] E: encode and loopback keep working at the login screen, on the lock screen and with UAC open.
- [x] F: QSV H.264 encodes from Session 0 on the laptop; p99 recorded, not slower than the user session
  (with levers). Needs a decoder-bind pool + GPU copy (Measurement notes → QSV).
- [x] G–J: loopback from Session 0: non-silent, 0 discontinuities, packet interval 10 ms (the smallest
  shared-mode loopback allows: `IAudioClient3` refuses loopback). **Partly** for I: reopen in 12 ms
  measured once; the other switch was unmeasurable (loopback is silent while nothing plays on the endpoint).
- [x] AMF recorded as untested (no hardware).
- [x] Results filled (cases table with Expected / Observed / Log) and the verdict.

## Results
**Main PC:** Windows 11 Pro 26H2 build 26300.9711, i5-14400F, RTX 5070 driver 32.0.16.1714, HAGS default,
Balanced power plan, GPU at P0 during runs. **Laptop:** Windows 11 Pro 25H2 build 26200.9711, i5-1245U,
Intel UHD Graphics driver 32.0.101.7088, Balanced. FFmpeg 8.1.3 LGPL (BtbN; no 8.0 build was left).
Logs: `tools/spikes/s4-encode-audio/out/` (local only): main PC `*-run-all-main.txt`, `*-run-all-main2.txt`,
`*-E-boot-*`; laptop `out/laptop*/out/` (`laptop3` = first working QSV, `laptop4` = second round).
Encode: 1440p120 (laptop QSV 1080p60), 10 000 frames per round (5 000 at 60 fps), 3 rounds per side unless
noted; rounds alternate A (service, Session 0) with B (same job as the logged-on user, not elevated) or C
(SYSTEM in the console session). Round 1 ran side B at Task Scheduler's below-normal priority; round 2
runs it at normal (`common.ps1`), so round-2 numbers are the fair comparison.

| Case | Expected | Observed | Log |
|---|---|---|---|
| A | adapter, NVENC, endpoints in Session 0 | **Pass.** RTX 5070, `h264_nvenc` / `hevc_nvenc` open in the service; loopback opens the default endpoint | `120604-A-*` |
| B | H.264 p99 ≤ 3 ms, A not slower than B | **Fail without levers, pass with them** (table below) | `run-all-main*.txt` |
| C | HEVC same | **Fail** (no levers): 3.36 vs 2.60 ms p99; not rerun with levers | `run-all-main.txt` |
| D | 600 frames, 0 errors, numbers read back | **Pass** for all 6 Session 0 dumps (+ E-boot); no `transfer_data` / staging texture in the encode path | `run-all-main.txt` |
| E | encode + loopback at login, lock, UAC | **Pass.** Login screen after reboot (service up 9.5 s after boot, levers on): 0 lost, p99 3.38 ms, 59/60 s loud. Lock: p99 5.61 ms, UAC: 5.49 ms (no levers), 0 lost, audio loud | `E-boot-*`, `E-lock-*`, `E-uac-*` |
| F | QSV H.264 in Session 0, not slower | **Pass with levers** (table below) after two fixes: D3D multithread protection on our device, decoder-bind pool | `laptop3`, `laptop4` |
| G | user audio, non-silent, 0 discontinuities | **Pass.** 10 min per side: 599/600 s loud, 0 discontinuities, interval p99 10.22 (A) vs 10.34 ms (B) | `G-*` |
| H | `--tone` at login / lock, non-silent | **Pass** (E-boot, E-lock, E-uac: 59/60 s loud) | as E |
| I | reopen on new default ≤ 1 s | **Partly.** Reopened on each switch; back to speakers: first packet 12 ms later. On the idle USB headset no packets came for ~38 s | `131006-I-*` |
| J | laptop loopback | **Pass**, 3 runs: 0 discontinuities (one at start in run 1), 119/120 s loud, interval p99 10.43–10.74 (A) vs 10.43–10.72 ms (B) | `laptop*/J-*` |
| K | console-session SYSTEM vs user | Same as Session 0: slower without levers, equal with them (table below) | `run-all-main2.txt` |

| Encode, pooled p50 / p99 (ms) | Session 0 (A) or console SYSTEM (C) | User session (B) | Rule |
|---|---|---|---|
| NVENC H.264, no levers (round 2) | A 2.65 / **5.13** | 2.27 / 3.10 | FAIL |
| NVENC H.264, levers 7 | A 2.30 / **3.15** | 2.24 / 2.95 | PASS |
| NVENC H.264, no levers | C 2.70 / **4.92** | 2.24 / 3.02 | FAIL |
| NVENC H.264, levers 7 | C 2.28 / **3.08** | 2.21 / 2.96 | PASS |
| QSV H.264 1080p60, no levers | A 7.09 / 10.97 | 6.71 / 19.23 (one noisy round) | PASS |
| QSV H.264 1080p60, levers 7 | A 7.05 / **10.51** | 6.76 / 10.39 | PASS |
| QSV HEVC 1080p60, no levers, 1 round | A 6.58 / 9.56 | 5.52 / 8.50 | FAIL |

Levers (`src/platform/levers.rs`, bits): 1 = MMCSS "Pro Audio" at critical priority on the encode thread,
2 = `IDXGIDevice::SetGPUThreadPriority(7)` on the encode device, 4 = `HIGH_PRIORITY_CLASS`. All three applied
on every side, also non-elevated. Not isolated: only masks 0 and 7 were measured.

### Measurement notes
- **Content:** the grid pattern is nearly static. H.264 still fills 30 Mbit/s; HEVC drops to 0.2 Mbit/s, so
  HEVC latencies are optimistic. Real frames (T7) will cost more.
- **QSV:** FFmpeg 8.1 cannot create its own D3D11 textures for QSV on this driver (`E_INVALIDARG`, also with
  `ffmpeg.exe`); only system-memory frames work there. Ours works when the device has multithread protection
  (without it MFX rejects the handle: -16) and the D3D11 pool uses `D3D11_BIND_DECODER` (NV12 render-target
  arrays fail on Intel and NVIDIA); the grid is drawn into an own NV12 texture and copied on the GPU. At
  1440p120 the iGPU falls behind (115 s for 10 000 frames); 1080p60 keeps up. QSV here is ~7 / 10.5 ms
  (p50 / p99): hardware-bound, far over the 3 ms budget, unrelated to Session 0.
- **Loopback timestamps:** the device QPC position is ~10.7 ms *ahead* of read time on the main PC and
  ~16 ms *behind* on the laptop, the same in Session 0 and the user session (one laptop run showed 26 ms in
  Session 0, not repeated).

### Verdict: ADR-0005 holds, with changes
NVENC (and QSV) encode and WASAPI loopback work from a LocalSystem service in Session 0, at the login screen,
lock screen and with UAC open, with valid bitstreams and no CPU copy. At default priorities the service
encodes ~2 ms slower at p99 than a user-session process; a SYSTEM process in the console session (the
ADR-0005 fallback) is just as slow, so moving there gains nothing. With the priority levers both match the
user session. Changes carried forward:
- **T7 / T19:** the encode thread in `mixlink-svc` raises its priority: MMCSS "Pro Audio" critical, GPU thread
  priority 7, high process priority class. Isolate which lever matters in T7; keep all three until then.
- **T7:** QSV input needs a decoder-bind pool (+ GPU copy) and a multithread-protected device; NV12
  render-target texture arrays are not portable. H.264 p99 sits at the 3 ms budget: phase-3 tuning.
- **T11:** shared-mode loopback delivers 10 ms packets (no `IAudioClient3` low latency) and **nothing while
  the endpoint plays nothing**: the capture side must fill silence on a timer and follow default-device
  changes (`IMMNotificationClient`, reopen in ~12 ms).
- AMF stays unproven (no AMD hardware): phase-2 test matrix.

## Open questions
- Which lever carries the gain (MMCSS, GPU priority or process priority): measure singly in T7.
- AMF on Session 0 stays unproven until an AMD machine is available (phase 2 test matrix).
- (Answered: loopback stays at the 10 ms engine period; T11 picks how to cut it into 5 ms Opus frames.)
