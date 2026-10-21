#!/usr/bin/env bash
# Checks docs structure:
# - every doc under docs/ is listed in docs/README.md (specs/ and decisions/ are listed as folders)
# - every spec has Status + Plan lines and a row in docs/specs/README.md
# - every ADR has a Status line
# - every crate/app folder has a module card CLAUDE.md
# - docs/plans/ is ignored and nothing in it is tracked
source "$(dirname "$0")/lib.sh"

index="docs/README.md"
while IFS= read -r f; do
  rel="${f#docs/}"
  case "$rel" in
    README.md | plans/* | specs/* | decisions/* | roadmap/phase-*) continue ;;
  esac
  grep -qF "\`$rel\`" "$index" || err "$f: not listed in $index"
done < <(repo_files | grep '^docs/.*\.md$')

spec_index="docs/specs/README.md"
while IFS= read -r f; do
  name="$(basename "$f" .md)"
  [[ "$name" == README || "$name" == _template ]] && continue
  [[ "$name" =~ ^[0-9]{4}-[a-z0-9-]+$ ]] || err "$f: name must be NNNN-kebab-case.md"
  grep -q '^\*\*Status:\*\* \(draft\|approved\|in-progress\|done\)' "$f" ||
    err "$f: missing '**Status:** draft|approved|in-progress|done'"
  grep -qE '^\*\*Date:\*\* [0-9]{4}-[0-9]{2}-[0-9]{2}' "$f" || err "$f: missing '**Date:** YYYY-MM-DD'"
  grep -qF "](../plans/$name.md)" "$f" || err "$f: missing plan link ](../plans/$name.md)"
  # Roadmap: a task row T<n> that links back to this spec; "phase 0" predates task IDs.
  roadmap="$(grep -m1 -E '^\*\*Roadmap:\*\* ' "$f" | sed 's/^\*\*Roadmap:\*\* //')"
  if [[ "$roadmap" =~ ^(T[0-9]+)$ ]]; then
    grep -qE "^\| ${BASH_REMATCH[1]} \|.*\]\(\.\./specs/$name\.md\)" docs/roadmap/phase-*.md ||
      err "$f: roadmap row $roadmap must exist and link ../specs/$name.md"
  elif [[ "$roadmap" == "phase 0" ]]; then
    grep -qF "](../specs/$name.md)" docs/roadmap/phase-0-foundation.md ||
      err "$f: phase-0-foundation.md must link ../specs/$name.md"
  else
    err "$f: missing '**Roadmap:** T<n>'"
  fi
  grep -qF "$name" "$spec_index" || err "$f: not listed in $spec_index"
done < <(repo_files | grep '^docs/specs/.*\.md$')

while IFS= read -r f; do
  grep -q '^\*\*Status:\*\* \(proposed\|accepted\|superseded\)' "$f" ||
    err "$f: missing '**Status:** proposed|accepted|superseded ...'"
done < <(repo_files | grep '^docs/decisions/.*\.md$')

for d in crates/*/ apps/*/ tools/; do
  [[ -d "$d" ]] || continue
  [[ -f "${d}CLAUDE.md" ]] || err "${d}: missing module card CLAUDE.md"
done

git check-ignore -q docs/plans/probe.md || err "docs/plans/ is not gitignored"
tracked_plans="$(git ls-files docs/plans)"
[[ -z "$tracked_plans" ]] || err "docs/plans/ has tracked files: $tracked_plans"

finish "check-docs"
