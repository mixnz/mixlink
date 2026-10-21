#!/usr/bin/env bash
# Asks CI about a branch: runs the gate, pushes the branch, dispatches ci.yml on it.
#   scripts/ask-ci.sh                 current branch, every job
#   scripts/ask-ci.sh some-branch     a branch by name (pushed without the gate)
#   scripts/ask-ci.sh --watch         request, then wait for the verdict (watch-ci.sh)
#   scripts/ask-ci.sh --jobs test     one job group only: docs | lint | test
#   scripts/ask-ci.sh --no-gate       skip the local gate (for CI-only changes)
#
# ci.yml runs only on dispatch and on `v*` tags, so every branch has to ask.
# Push comes first: `gh workflow run --ref` builds what the remote has, not the local HEAD.
# A workflow is dispatchable only once ci.yml with `workflow_dispatch` is on remote master.
# A narrowed run (--jobs) is not an answer about the workspace: ask for `all` before believing it.
#
# Exit: 0 run requested, 1 something refused, 64 misuse of this script.
set -uo pipefail

groups="all docs lint test"
watch=0 gate=1 branch="" jobs="" want_jobs=0
for arg in "$@"; do
  if ((want_jobs)); then
    jobs="$arg" want_jobs=0
    continue
  fi
  case "$arg" in
    --watch) watch=1 ;;
    --no-gate) gate=0 ;;
    --jobs) want_jobs=1 ;;
    --jobs=*) jobs="${arg#--jobs=}" ;;
    -*) echo "unknown option: $arg" >&2; exit 64 ;;
    *) branch="$arg" ;;
  esac
done

if ((want_jobs)); then
  echo "--jobs needs a group: $groups" >&2
  exit 64
fi
case " $groups " in
  *" ${jobs:-all} "*) ;;
  *) echo "unknown job group: $jobs (one of: $groups)" >&2; exit 64 ;;
esac

current="$(git branch --show-current)"
[[ -n "$branch" ]] || branch="$current"
if [[ -z "$branch" ]]; then
  echo "not on a branch, and none was named" >&2
  exit 64
fi

if ((gate)); then
  if [[ "$branch" != "$current" ]]; then
    echo "not running the gate: $branch is not the branch checked out here" >&2
  else
    if [[ -n "$(git status --porcelain --untracked-files=no)" ]]; then
      echo "warning: working tree has uncommitted changes that will not be pushed" >&2
    fi
    bash "$(dirname "${BASH_SOURCE[0]}")/gate.sh" || {
      echo "nothing was pushed; fix the above, or pass --no-gate" >&2
      exit 1
    }
  fi
fi

git push origin "$branch" || exit 1

if [[ -n "$jobs" && "$jobs" != "all" ]]; then
  gh workflow run ci.yml --ref "$branch" -f jobs="$jobs" || exit 1
  echo "requested a run on $branch — $jobs only; ask for all before believing a green answer"
else
  gh workflow run ci.yml --ref "$branch" || exit 1
  echo "requested a run on $branch"
fi

if ((watch)); then
  exec bash "$(dirname "${BASH_SOURCE[0]}")/watch-ci.sh"
fi
echo "watch it with: bash scripts/watch-ci.sh"
