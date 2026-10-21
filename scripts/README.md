# Scripts

Bash scripts (Git Bash on Windows). Each script <= 150 lines; shared helpers in `lib.sh`.

| Script | Purpose |
|---|---|
| `gate.sh` | Quality gate: all checks + `cargo fmt/clippy/test`. `--fast` skips clippy/test. |
| `build.sh` | `debug` (default) or `release` build; release copies binaries to `dist/`. |
| `check-text.sh` | LF endings, final newline, no tabs in md/toml/yml. |
| `check-lines.sh` | File size limits (`docs/conventions/file-limits.md`). |
| `check-links.sh` | Markdown links and `` `docs/...md` `` references exist (plans exempt). |
| `check-docs.sh` | Doc index, spec/ADR headers, module cards, `docs/plans/` ignored. |
| `setup-hooks.sh` | Enables `.githooks/` (pre-commit: fast gate, pre-push: full gate + branch name). |

Run from anywhere: `bash scripts/gate.sh`. Scripts `cd` to the repo root themselves.
