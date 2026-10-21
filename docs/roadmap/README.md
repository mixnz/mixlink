# Roadmap

The path from empty repo to a shippable MixLink, as phases of small tasks.
Work is picked from here: one task → one spec → one plan → `/execute` → `/land`.

## Phases
| Phase | File | Goal | Status |
|---|---|---|---|
| 0 | [phase-0-foundation.md](phase-0-foundation.md) | Repo, rules, gate, CI, empty workspace | done |
| 1 | [phase-1-mvp-lan.md](phase-1-mvp-lan.md) | Test on two real PCs: video, keyboard/mouse, audio, gamepad, desktop UI | todo |
| 2 | [phase-2-release.md](phase-2-release.md) | Installer, signing, updates, first release | todo |
| 3 | [phase-3-optimize.md](phase-3-optimize.md) | FPS-grade latency under real network conditions | todo |
| 4 | [phase-4-internet.md](phase-4-internet.md) | Same experience over the internet | todo |

**Current phase:** 1. **Next task:** the first `todo` row in the current phase whose `Needs` are all `done`.

## Task rows
Each phase file has one table: `| ID | Task | Crates | Needs | Spec | Status |`.
- **ID** `T<n>`, global across phases (`T7` is unique), never renumbered; a new task takes the next free number.
- **Needs**: IDs that must be `done` first (`—` if none).
- **Spec**: link to `docs/specs/NNNN-*.md` once written, `—` before.
- **Status**: `todo` → `spec` (spec written) → `wip` (`/execute` running) → `done` (landed). `dropped` keeps the row.

## Rules
- A task is sized for one spec: one reviewable branch, roughly 1–6 plan tasks. Too big → split into rows.
- Phases after the current one list only their goal; their tasks are split with the user when the phase starts.
- Writing a spec: set the row to `spec` and link it; the spec's `**Roadmap:**` line names the row ID.
- `/execute` sets the row to `wip` in its first task commit and `done` in its last one.
- A phase is `done` when every row is `done` or `dropped`; then bump **Current phase**.
- Adding, splitting, reordering or dropping rows is a roadmap change: ask the user first.
