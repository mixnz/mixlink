# Scripts

Bash scripts (Git Bash on Windows). Each script <= 150 lines; shared helpers in `lib.sh`.

| Script | Purpose |
|---|---|
| `gate.sh` | Quality gate: all checks + UI + `cargo fmt/clippy/test`. `--fast` skips UI/clippy/test; `--ui` = UI only; `--checks`, `--cargo lint\|test` are the CI jobs (cargo modes run UI first). |
| `check-ui.sh` | npm workspace: `npm ci` when the lockfile changed, then lint, typecheck, test, build (`ui/dist` for Tauri). |
| `build.sh` | `debug` (default) or `release` build; release runs `check-ui`, embeds the UI (`custom-protocol`) and copies host, client, stream exes to `dist/`. |
| `ask-ci.sh` | Gate, push branch, dispatch `ci.yml` (CI runs only on dispatch and `v*` tags). `--jobs`, `--watch`. |
| `watch-ci.sh` | Watch the run for HEAD; `--once` = report and exit (2 = running). |
| `ci-skippable.sh` | `BASE [TARGET]`: exit 0 when only `*.md` / Rust comment lines changed (used by `/land`). |
| `ci-log-extract.sh` | Short failure extract of a run (called by `watch-ci.sh`); full log saved to a file. |
| `check-text.sh` | LF endings, final newline, no tabs in md/toml/yml. |
| `check-lines.sh` | File size limits (`docs/conventions/file-limits.md`). |
| `check-links.sh` | Markdown links and `` `docs/...md` `` references exist (plans exempt). |
| `check-docs.sh` | Doc index, spec/ADR headers, module cards, `docs/plans/` ignored. |
| `check-deps.sh` | Crate dependency direction (allow-table mirrors `CLAUDE.md`), all dependency sections. |
| `setup-hooks.sh` | Enables `.githooks/` (pre-push: branch name check only; the gate runs in `/land`). |

Run from anywhere: `bash scripts/gate.sh`. Scripts `cd` to the repo root themselves.
