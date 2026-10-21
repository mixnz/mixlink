---
description: Run the CI loop until every check is green — watch, classify, fix, gate, commit, ask again
argument-hint: [branch or note, optional]
allowed-tools: Bash(bash scripts/ask-ci.sh:*), Bash(bash scripts/watch-ci.sh:*), Bash(gh run:*), Bash(git rev-parse:*), Bash(git status:*)
disable-model-invocation: true
---

# Watching CI until green

Branch: !`git rev-parse --abbrev-ref HEAD`
HEAD: !`git rev-parse HEAD`
Uncommitted: !`git status --porcelain`

User's note for this round (may be empty): $ARGUMENTS

**Invoking this command authorizes** committing, pushing and dispatching a run on the working branch, once per round,
without asking again. Nothing else: no force push, no PR, no merge, no tag, nothing committed or pushed on `master`.

## The loop

**Before asking, check whether this commit already did.** The question is about **HEAD**, not the branch.
`bash scripts/watch-ci.sh --once`: exit 2 = a run is already in flight → go to step 2; exit 1 printing `no run for <sha>`
means nothing was asked yet (not red). For another branch by name, find its HEAD in
`gh run list --branch <name> --json databaseId,headSha,status` and pass that id to `bash scripts/watch-ci.sh <id>`.

1. `bash scripts/ask-ci.sh` — runs the gate, pushes the branch, dispatches `ci.yml` (it runs on nothing else but
   `v*` tags). `--watch` chains step 2. `--jobs docs|lint|test` narrows the run; **a narrowed answer is not an answer
   about the workspace** — ask for `all` before believing it.
2. `bash scripts/watch-ci.sh` — picks the run by HEAD's SHA. A run outlives a foreground command: run it in the
   background or poll `--once`. Exit 0 green, 1 red, 2 running, 64 misuse.
3. **Red** → classify below, fix one thing, gate it, one commit, back to step 1.
4. **Green on an `all` run** (every job settled) → stop. Report run id, branch, one line per fix, which reds were flakes.

**Red on `master`:** first `git checkout -b <short-name-for-the-failure>` (never "claude" in any casing) and run the
loop there. `master` stays unpushed.

## Classify before fixing

The printed block is an extract. Read the full log whose path is the last line printed, and grep that job's lines
(`error[`, `ERROR`, `FAILED`, `Process completed with exit code`). `EXTRACT_LINES=500` keeps more on screen.

| What CI says | What it is | What to do |
| --- | --- | --- |
| `workflow ... not found` / `does not have 'workflow_dispatch'` on `gh workflow run` | `ci.yml` with dispatch is not on remote `master` yet | stop and tell the user; pushing `master` is their call |
| a job with no failing step — killed at setup, cancelled, runner lost | infrastructure | `gh run rerun <id> --failed` |
| a step needing a repository secret or a person's setting | not yours to fix | stop and say so |
| `docs` job red, gate green locally | the gate runs on the working tree; an uncommitted fix | commit it, ask again |
| the same failure twice on the same commit, identically | a real failure | fix it |

## Gate the fix before pushing

Run `bash scripts/gate.sh` (or `--fast` plus `cargo test -p <crate>` while iterating). `ask-ci.sh` runs the full gate
again before pushing. Locally green is not CI green: this machine builds one profile on one OS.

## Red flags — stop, do not push

Weakening a test (delete, `#[ignore]`, `#[allow(...)]`), `--no-verify`, `todo!()` on an unsupported path, editing an
accepted ADR, raising a limit in `docs/conventions/file-limits.md` to pass, a commit or push on `master`, or
`gh pr create` / `gh pr merge` / `git merge` / `git rebase` / a `v*` tag — none are the loop.

| Excuse | Reality |
| --- | --- |
| "The extract shows the error" | It shows *an* extract. The full log is on disk; read it. |
| "It is green locally" | Local is one OS, one profile. Not evidence. |
| "Just a fmt fix, push it" | Gate it anyway; it is faster than one CI round. |
| "The run before this one was green" | `watch-ci.sh` answers about HEAD only. |
| "`--jobs test` came back green" | Only an `all` run ends the loop. |
| "The fix is one line, `master` can take it" | A red `master` gets a branch; the loop never commits to `master`. |
| "The same fix did not work, try another" | Twice with nothing learned → stop and report what each round learned. |
| "It is green, I will open the PR" | Green ends the loop. PR/merge is the user's call (or `/land`). |
