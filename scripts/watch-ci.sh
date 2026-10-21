#!/usr/bin/env bash
# Watches the CI run for HEAD and, on failure, prints an extract of the failing logs.
#   scripts/watch-ci.sh                 run for HEAD on the current branch
#   scripts/watch-ci.sh 12345678        a run by id
#   scripts/watch-ci.sh --once          report now and exit; 2 = still running
#   INTERVAL=60 / TIMEOUT=7200 / EXTRACT_LINES=500 tune polling and output
#
# The run is picked by HEAD's SHA, never by recency (the newest run may be the previous push).
# Filters use `gh --jq` because Git Bash ships no jq.
# A full run can outlive a foreground command: run this in the background or poll with --once.
#
# Exit: 0 success, 1 failure / no run, 2 still running (--once only), 64 misuse.
set -uo pipefail

interval="${INTERVAL:-30}"
timeout="${TIMEOUT:-3600}"
branch="$(git branch --show-current)"

once=0 run=""
for arg in "$@"; do
  case "$arg" in
    --once) once=1 ;;
    -*) echo "unknown option: $arg" >&2; exit 64 ;;
    *) run="$arg" ;;
  esac
done

if [[ -z "$run" ]]; then
  sha="$(git rev-parse HEAD)"
  find_run() {
    gh run list --branch "$branch" --limit 20 --json databaseId,headSha \
      --jq ".[] | select(.headSha == \"$sha\") | .databaseId" | head -n 1
  }
  run="$(find_run)"
  # A dispatched run takes a few seconds to appear; --once never waits.
  if ((once == 0)); then
    waited=0
    while [[ -z "$run" ]] && ((waited < 120)); do
      ((waited == 0)) && echo "waiting for a run on ${sha:0:7}…"
      sleep 10
      waited=$((waited + 10))
      run="$(find_run)"
    done
  fi
  if [[ -z "$run" ]]; then
    echo "no run for ${sha:0:7} on $branch — request one with: bash scripts/ask-ci.sh" >&2
    exit 1
  fi
fi

repository="$(gh repo view --json nameWithOwner --jq .nameWithOwner)"
echo "watching run $run on $branch — https://github.com/$repository/actions/runs/$run"

reported="" status="" total="?" settled=0 failed=0 failed_names=""

# Prints jobs that settled since the last poll; recomputes run totals every time.
poll() {
  local snapshot field name
  snapshot="$(gh run view "$run" --json status,jobs \
    --jq '"status\t" + .status, "total\t" + (.jobs | length | tostring),
          (.jobs[] | select(.status == "completed") | .conclusion + "\t" + .name)')"
  settled=0 failed=0 failed_names=""
  while IFS=$'\t' read -r field name; do
    [[ -z "$name" ]] && continue
    case "$field" in
      status) status="$name"; continue ;;
      total) total="$name"; continue ;;
    esac
    settled=$((settled + 1))
    if [[ "$field" != success && "$field" != skipped ]]; then
      failed=$((failed + 1))
      failed_names="${failed_names:+$failed_names, }$name"
    fi
    [[ "$reported" == *"|$name|"* ]] && continue
    reported="$reported|$name|"
    case "$field" in
      success) printf '  ok       %s\n' "$name" ;;
      skipped) printf '  skipped  %s\n' "$name" ;;
      cancelled) printf '  stopped  %s\n' "$name" ;;
      *) printf '  FAILED   %s\n' "$name" ;;
    esac
  done <<<"$snapshot"
}

if ((once)); then
  poll
  if [[ "$status" != completed ]]; then
    summary="run $run: $status — $settled/$total jobs settled"
    ((failed > 0)) && summary="$summary, $failed failed: $failed_names"
    echo "$summary"
    exit 2
  fi
else
  elapsed=0
  while true; do
    poll
    [[ "$status" == completed ]] && break
    if ((elapsed >= timeout)); then
      echo "run $run still $status after ${timeout}s — stopped watching, the run continues" >&2
      exit 1
    fi
    sleep "$interval"
    elapsed=$((elapsed + interval))
  done
fi

conclusion="$(gh run view "$run" --json conclusion --jq '.conclusion')"
skipped="$(gh run view "$run" --json jobs \
  --jq '[.jobs[] | select(.conclusion == "skipped") | .name] | join(", ")' 2>/dev/null)"

if [[ "$conclusion" == success ]]; then
  echo "run $run: success"
  if [[ -n "$skipped" ]]; then
    echo "  but this run was narrowed: $skipped never ran — ask for all before believing it."
  fi
  exit 0
fi

echo
if ((failed > 0)); then
  echo "run $run: $conclusion — $failed of $total jobs failed: $failed_names"
else
  echo "run $run: $conclusion"
fi
exec bash "$(dirname "${BASH_SOURCE[0]}")/ci-log-extract.sh" "$run" "$repository"
