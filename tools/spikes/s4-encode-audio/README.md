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
- `s4-ctl check`: adapters, FFmpeg version, which hardware encoders the build has.
