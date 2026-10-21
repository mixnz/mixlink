# Phase 0 — Foundation

**Goal:** a repo where AI sessions can work cheaply and every rule is enforced by tooling.
**Done when:** gate + CI run on an empty, compiling workspace.

Done before task IDs existed, so these rows have no `T` number; spec 0001 names this phase instead.

| ID | Task | Crates | Needs | Spec | Status |
|---|---|---|---|---|---|
| — | Repo layout, CLAUDE.md, module cards, architecture docs, ADRs | — | — | — | done |
| — | Check scripts, `gate.sh`, `build.sh`, git hooks | — | — | — | done |
| — | Dispatch-only CI, `/execute`, `/land`, `/watch-ci` | — | — | — | done |
| — | Cargo workspace bootstrap | all | — | [0001](../specs/0001-workspace-bootstrap.md) | done |
