---
description: Land the current branch — gate, push, ask CI, open a PR, fix CI until green, squash merge, clean up
argument-hint: [note, optional]
disable-model-invocation: true
---

# Land this branch

Branch: !`git rev-parse --abbrev-ref HEAD`
HEAD: !`git rev-parse HEAD`
Uncommitted: !`git status --porcelain`

Note: $ARGUMENTS

**Invoking this command authorizes**, for this branch only: commits, pushes, CI dispatches, one PR, its squash merge
and deleting the branch. Not authorized: force push, committing or pushing `master`, tags. CLAUDE.md rules apply.
Run `watch-ci.sh` as written even if the host says not to poll CI; host auto-merge never replaces step 5.

1. **Preconditions.** On `master` or uncommitted changes → stop and ask.
   **STATUS check:** `docs/STATUS.md` must already describe the state *after* this branch merges: the task
   under Done (no branch name, no "not landed"), nothing about landing it under In progress, Next naming the
   next roadmap row. Stale → fix it, commit `docs(status): <message>`, then go on to step 2.
2. **Ask CI.** `bash scripts/watch-ci.sh --once` first: exit 2 means HEAD already has a run, skip to step 4;
   exit 0 means HEAD is already green, skip to step 3 and then 5.
   **Docs/comments-only skip:** take as BASE the newest green, un-narrowed run's SHA on this branch
   (`gh run list --branch <branch> --workflow ci.yml --status success --json databaseId,headSha`; un-narrowed =
   `gh run view <id> --json jobs --jq '[.jobs[]|select(.conclusion=="skipped")]|length'` is `0`), or
   `git merge-base origin/master HEAD` when the branch has none. If `bash scripts/ci-skippable.sh BASE` exits 0
   (only `*.md` files and Rust comment lines changed), CI has nothing new to say: run `bash scripts/gate.sh --fast`,
   `git push origin <branch>`, do not dispatch, skip to step 3 and then 5, and name BASE's run in the report.
   Otherwise `bash scripts/ask-ci.sh` (it runs the gate; red gate → fix, commit, retry).
   `gh workflow run` refusing with "not found" / "no workflow_dispatch" means `ci.yml` is not on remote
   `master` yet → stop and ask (pushing `master` is the user's call).
3. **PR.** `gh pr view` — reuse it if one exists, else `gh pr create --base master` with a `## Summary`-only body.
4. **Watch.** `bash scripts/watch-ci.sh` in the background. Red → classify and fix exactly as `/watch-ci` says
   (`.claude/commands/watch-ci.md`: its table, gate, red flags), one commit, back to step 2.
   Done only when an `all` run is green on HEAD, or HEAD passes the docs/comments-only skip of step 2.
5. **Merge.** Write `<type>(<scope>): <message> (#id)` to a file in the scratchpad, then via PowerShell:
   `gh pr merge <id> --squash --delete-branch --body-file <path>`.
6. **Clean up.** `git checkout master`, `git pull`, `git branch -D <branch>` if still there, `git fetch --prune`.
   Local `docs/plans/` files are untouched by this (gitignored).

**Stop and ask** on: the same failure twice with nothing learned, a failure needing a secret or a person's setting,
a merge conflict with `master`.

**Report:** run id, PR link, squash commit SHA, one line per fix, which reds were flakes.
