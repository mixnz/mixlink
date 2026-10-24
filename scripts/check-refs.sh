#!/usr/bin/env bash
# Checks that code comments cite specs/ADRs, never roadmap task/spike ids (T6, S2b) or docs/roadmap/.
# Rule: docs/conventions/coding.md -> "References in code". tools/spikes/ is throwaway and exempt.
source "$(dirname "$0")/lib.sh"

# A task/spike id or a roadmap path, after a comment marker on the same line.
ref='(\b[TS][0-9]+[a-z]?\b|docs/roadmap|roadmap/phase-)'
slash_comment="(//|/\\*|^\\s*\\*).*$ref"
hash_comment="#.*$ref"

mapfile -t files < <(repo_files | grep -vE '^tools/spikes/' | grep -E '^(crates|apps|drivers|tools)/')
mapfile -t slash_files < <(printf '%s\n' "${files[@]}" | grep -E '\.(rs|ts|tsx|js|mjs|cpp|c|h|hpp|css)$')
mapfile -t hash_files < <(printf '%s\n' "${files[@]}" | grep -E '\.(toml|ps1|sh|ya?ml)$')

report() {
  local pattern="$1"
  shift
  (($# == 0)) && return
  while IFS= read -r hit; do
    err "$hit"
  done < <(grep -nIP -- "$pattern" "$@")
}

report "$slash_comment" "${slash_files[@]}"
report "$hash_comment" "${hash_files[@]}"

((ERRORS > 0)) && echo "Name the feature or cite a spec/ADR instead (docs/conventions/coding.md)."
finish "check-refs"
