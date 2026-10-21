#!/usr/bin/env bash
# Prints a short extract of a failed CI run's logs; the full log is saved to a file.
#   scripts/ci-log-extract.sh <run-id> [owner/repo]
# Called by watch-ci.sh. The reader is usually an agent, so: the full log goes to a file,
# identical failures across jobs are printed once, and any cut says how much was cut.
# Always exits 1 (it only runs for a failed run).
set -uo pipefail

run="${1:?usage: ci-log-extract.sh <run-id> [owner/repo]}"
repository="${2:-$(gh repo view --json nameWithOwner --jq .nameWithOwner)}"
extract_lines="${EXTRACT_LINES:-200}"
log="${TMPDIR:-/tmp}/mixlink-ci-$run.log"

echo "the failing steps:"
echo
gh run view "$run" --log-failed >"$log" 2>/dev/null

if [[ ! -s "$log" ]]; then
  # Jobs killed at setup / cancelled have no failing step; use the tail of their own log.
  for job in $(gh run view "$run" --json jobs \
    --jq '.jobs[] | select(.conclusion != "success" and .conclusion != "skipped") | .databaseId'); do
    gh run view "$run" --job "$job" --log 2>/dev/null | tail -n 80 >>"$log"
  done
fi
if [[ ! -s "$log" ]]; then
  echo "  (no log — read it at https://github.com/$repository/actions/runs/$run)"
  exit 1
fi

# Patterns that may appear inside Rust paths are anchored to line start, so a passing
# `test error::tests::x ... ok` line is not reported as a failure.
failure_re='##\[error\]|^error(\[E[0-9]+\])?:|^Error: |^ERROR |^FAIL |GATE FAILED|panicked at|assertion.*failed|^ *--> |^test result: FAILED|^failures:|Process completed with exit code'

# Strips gh's "job<TAB>step<TAB>timestamp " prefix (optionally BOM-led) and ANSI colours,
# including Actions' literal "^[[1m" form.
clean_job() {
  awk -F'\t' -v job="$1" '$1 == job' "$log" |
    sed -E $'s/^[^\t]*\t[^\t]*\t(\xef\xbb\xbf)?[0-9T:.Z-]+ //; s/\x1b\\[[0-9;]*[A-Za-z]//g; s/\\^\\[\\[[0-9;]*[A-Za-z]//g'
}

extract_job() {
  clean_job "$1" | grep -aE "$failure_re" -A 6 |
    grep -avE '^--$|^##\[(end)?group\]|^shell: |^env:$|^ +[A-Z_][A-Z_0-9]*: '
}

# Same failure = same failure lines once digits and path separators are removed.
fingerprint_job() {
  clean_job "$1" | grep -aE "$failure_re" -A 1 | tr '\134' '/' | sed -E 's/[0-9]+//g' |
    sort -u | cksum | cut -d' ' -f1
}

body="$log.extract"
: >"$body"
seen=""
jobs_in_log="$(cut -f1 "$log" | sort -u)"

while IFS= read -r job; do
  [[ -z "$job" ]] && continue
  block="$(extract_job "$job")"
  [[ -z "$block" ]] && continue
  fp="$(fingerprint_job "$job")"
  if [[ "$seen" == *"|$fp="* ]]; then
    twin="${seen#*|"$fp"=}"
    printf '=== %s — the same failure as %s ===\n\n' "$job" "${twin%%|*}" >>"$body"
    continue
  fi
  seen="$seen|$fp=$job|"
  printf '=== %s ===\n%s\n\n' "$job" "$block" >>"$body"
done <<<"$jobs_in_log"

lines="$(wc -l <"$body")"
if ((lines > extract_lines)); then
  head -n "$extract_lines" "$body"
  echo "… $((lines - extract_lines)) more lines cut — raise EXTRACT_LINES or read the full log"
else
  cat "$body"
fi
echo "full log: $log"
exit 1
