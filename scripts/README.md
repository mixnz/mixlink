# Scripts

Bash scripts (Git Bash on Windows). Each script <= 150 lines; shared helpers in `lib.sh`.

| Script | Purpose |
|---|---|
| `gate.sh` | Quality gate: all checks + UI + drivers + `cargo fmt/clippy/test`. `--fast` skips UI/drivers/clippy/test; `--ui` = UI only; `--checks`, `--cargo lint|test` are the CI jobs (cargo modes run UI first). |
| `check-ui.sh` | npm workspace: `npm ci` when the lockfile changed, then lint, typecheck, test, build (`ui/dist` for Tauri). |
| `build.sh` | `debug` (default) or `release` build; release runs `check-ui`, embeds the UI (`custom-protocol`), copies host, client, stream, svc exes and FFmpeg DLLs to `dist/`, builds the drivers into `dist/drivers/<name>/` (unsigned). A file in use in `dist/` (running service or tray) fails the build. |
| `check-drivers.sh` | Builds both UMDF drivers (`drivers/build.ps1`) and runs `infverif /w`; no WDK toolset = WARN and skip (`MIXLINK_REQUIRE_WDK=1` = error, CI `drivers` job). |
| `ask-ci.sh` | Gate, push branch, dispatch `ci.yml` (CI runs only on dispatch and `v*` tags). `--jobs`, `--watch`. |
| `watch-ci.sh` | Watch the run for HEAD; `--once` = report and exit (2 = running). |
| `ci-skippable.sh` | `BASE [TARGET]`: exit 0 when only `*.md` / Rust comment lines changed (used by `/land`). |
| `ci-log-extract.sh` | Short failure extract of a run (called by `watch-ci.sh`); full log saved to a file. |
| `check-text.sh` | LF endings, final newline, no tabs in md/toml/yml. |
| `check-lines.sh` | File size limits (`docs/conventions/file-limits.md`). |
| `check-links.sh` | Markdown links and `` `docs/...md` `` references exist (plans exempt). |
| `check-docs.sh` | Doc index, spec/ADR headers, module cards, `docs/plans/` ignored. |
| `check-deps.sh` | Crate dependency direction (allow-table mirrors `CLAUDE.md`), all dependency sections. |
| `check-refs.sh` | Code comments cite specs/ADRs, never roadmap task/spike ids or `docs/roadmap/` (`tools/spikes/` exempt). |
| `make-test-stream.sh` | Test bitstreams for `mixlink-stream --play` (`out/test-<size>-<fps>.h264/.hevc`, NVENC or `--encoder qsv`), checked with `ffprobe`. |
| `ci-ffmpeg.sh` | CI only: downloads the pinned FFmpeg zip (`FFMPEG_URL`) into `FFMPEG_DIR` unless the cache has it. |
| `setup-hooks.sh` | Enables `.githooks/` (pre-push: branch name check only; the gate runs in `/land`). |

Run from anywhere: `bash scripts/gate.sh`. Scripts `cd` to the repo root themselves.

FFmpeg (spec 0013): `mixlink-codec` builds against FFmpeg 8.1 LGPL shared (BtbN zip unpacked to
`C:\tools\ffmpeg`) and needs LLVM's `libclang.dll`; `.cargo/config.toml` holds both default paths.
The gate puts `C:\tools\ffmpeg\bin` on `PATH`; for plain `cargo test` add it to your user `PATH` once.
