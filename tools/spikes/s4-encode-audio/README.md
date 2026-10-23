# Spike S4: hardware encode and WASAPI loopback from Session 0 (throwaway)

Spec: [0007](../../../docs/specs/0007-spike-encode-audio-session0.md).
Standalone Cargo package (not in the main workspace). Output (logs, CSV, bitstreams) goes to `out/` (gitignored).

## Prerequisites (installed by hand; scripts never download)
- LLVM (libclang, build only): `winget install LLVM.LLVM`.
- FFmpeg 8.x LGPL shared build (BtbN `ffmpeg-n8.1-latest-win64-lgpl-shared-8.1.zip`) unpacked to `C:\tools\ffmpeg`
  (`bin\*.dll`, `bin\ffprobe.exe`, `include\`, `lib\*.lib`).

## Build (Git Bash)
- `export FFMPEG_DIR='C:\tools\ffmpeg' LIBCLANG_PATH='C:\Program Files\LLVM\bin'`
- `cargo build --release --manifest-path tools/spikes/s4-encode-audio/Cargo.toml`
- Copy `C:\tools\ffmpeg\bin\*.dll` next to the exes in `target/release/` (`install-svc.ps1` does it too).

## Pieces
- `s4-ctl check`: adapters, FFmpeg version, which hardware encoders the build has, render endpoints.
- `s4-ctl verify <file>`: software-decodes a `.h264` / `.hevc` dump, counts frames and errors, reads the
  frame-number grid of frames 0, 299, 599.
- `s4-svc --console --case <c> --side B encode [--backend nvenc|qsv] [--codec h264|hevc] [--format bgra|nv12]
  [--size WxH] [--fps N] [--frames N]`: encode job in this process. Writes `out/<stamp>-<case>-<side>-encode.{txt,csv,h264|hevc}`.
- `s4-svc --console --case <c> --side B audio [--minutes N] [--tone]`: WASAPI loopback of the default output
  (MMCSS Pro Audio); reopens on a default-device change. `audio.csv` = packet interval (pooled A vs B); the
  summary also has the signed offset of the device QPC position, loud/quiet seconds and reopen gaps.
  `both` runs encode and audio at once. `--tone` plays the test tone from the same process.
- `s4-tone [--minutes 30]`: 1 kHz at -12 dBFS on the default output (user session).
- `s4-ctl pool <case> <encode|audio>`: pooled p50/p99/max per side (A, B, C) over `out/*-<case>-<side>-<job>.csv`
  and the "not slower" rule (side A or C vs B). Saved as `out/<stamp>-<case>-pool-<job>.txt`.

## Scripts (elevated PowerShell)
- `report.ps1`: Windows build, GPUs and drivers, HAGS, power plan, `nvidia-smi` state, `s4-ctl check`.
- `install-svc.ps1 [-AutoJob "<job args>"]`: service `MixLinkS4` (LocalSystem, demand start; automatic with
  `-AutoJob` for the reboot run). Copies FFmpeg DLLs next to the exes. `uninstall-svc.ps1` removes it.
- `run-case.ps1 -Case <c> -Job encode|audio|both [-Backend] [-Codec] [-Format] [-Size] [-Fps] [-Frames]
  [-Minutes] [-Tone] [-Rounds 3] [-Sides AB|A|B|C] [-Delay <s>]`: per round side A = service in Session 0,
  side B = same job as the logged-on user, not elevated (one-shot scheduled task); then `s4-ctl pool`.
- `-Sides C` (fallback probe, case K): the service starts the same job as SYSTEM in the active console session
  (`s4-svc --in-console`, ADR-0005 fallback layout).

## Session states (cases E, H)
1. `install-svc.ps1 -AutoJob "both --minutes 1 --tone --delay-seconds 30"`, reboot, wait at the login screen
   2 min, log in (case `E-boot`). Then `install-svc.ps1` again (demand start).
2. `run-case.ps1 -Case E-lock -Job both -Minutes 1 -Frames 7200 -Tone -Rounds 1 -Sides A -Delay 15`, Win+L within 15 s.
3. Same with `-Case E-uac`, opening a UAC prompt within 15 s.
