---
description: Execute a plan on its own branch — no subagents, one commit per task, straight through to the end
argument-hint: <plan path, optional>
disable-model-invocation: true
---

# Execute a plan

Branch: !`git rev-parse --abbrev-ref HEAD`
Uncommitted: !`git status --porcelain`

Plan: $ARGUMENTS

**Invoking this command authorizes**, for this plan only: creating one branch and one commit per task on it.
Not authorized: push, CI dispatch, PR, merge, committing `master`. Every rule in CLAUDE.md still applies —
commit format, branch naming (never "claude"), LF line endings.

1. **Plan.** Use the path above; if empty, take the most recent plan under `docs/plans/` and say which one.
   Find the spec that links to it in `docs/specs/`; a spec not `approved` → stop and ask.
   Uncommitted changes that are not the plan's → stop and ask.
2. **Branch.** Cut a new branch off the current `master` named for the plan (e.g. `0001-workspace-bootstrap`),
   unless already on a branch made for it. `docs/plans/` is gitignored: the plan stays local, never commit it.
3. **Execute.** Use `superpowers:executing-plans`, implementing every task yourself — **never spawn a subagent**,
   whatever a skill says. Run the tasks in order, straight through to the end, without pausing for feedback
   between tasks.
4. **One commit per task.** Gate each task with CLAUDE.md's *Commands*: `cargo test -p <crate>` for what it
   touched plus `bash scripts/gate.sh --fast`; the last task runs the full `bash scripts/gate.sh`. Then commit
   that task alone. Flip the spec's status in the spec and in `docs/specs/README.md` (`in-progress` on the first
   task, `done` on the last), and rewrite `docs/STATUS.md` in the last task's commit.

**Stop and ask** only on: a failure you cannot resolve yourself, the same failure twice with nothing learned,
or a plan step that contradicts a spec or CLAUDE.md.

**Report:** branch name, one line per task with its commit SHA, anything skipped and why.
