#!/usr/bin/env bash
# Says whether CI has nothing new to say about TARGET compared to BASE (a green, un-narrowed run's SHA).
#   scripts/ci-skippable.sh BASE [TARGET]     TARGET defaults to HEAD
# Skippable = every changed file is Markdown, or a Rust file whose changed lines are all
# `//` / `/*` comments or blank. Doc comments (`///`, `//!`) count only in files without ```
# fences, since fenced blocks are doctests. Anything else (other files, a code line,
# a block-comment line starting with `*`) needs CI — when unsure, ask CI.
# The local `gate.sh --fast` still runs: it covers links, limits and rustfmt on comments.
#
# Exit: 0 skippable, 1 CI needed, 64 misuse.
set -uo pipefail
cd "$(dirname "$0")/.." || exit 64

base="${1:-}"
target="${2:-HEAD}"
if [[ -z "$base" ]] || ! git rev-parse -q --verify "$base^{commit}" >/dev/null ||
  ! git rev-parse -q --verify "$target^{commit}" >/dev/null; then
  echo "usage: ci-skippable.sh BASE [TARGET] (both must be commits)" >&2
  exit 64
fi

need_ci() {
  echo "CI needed: $1"
  exit 1
}

mapfile -t changed < <(git diff --name-only --no-renames "$base" "$target")
((${#changed[@]} == 0)) && need_ci "no changes between $base and $target"

for f in "${changed[@]}"; do
  case "$f" in
    *.md) continue ;;
    *.rs) ;;
    *) need_ci "$f is not Markdown or Rust" ;;
  esac
  # A doc line inside a fenced block is doctest code, so files with fences get no doc-line pass.
  has_fence=0
  git show "$target:$f" 2>/dev/null | grep -qF '```' && has_fence=1
  while IFS= read -r line; do
    body="${line:1}"
    [[ "$body" == *'```'* ]] && need_ci "$f changes a doc-comment code fence (doctest)"
    [[ "$body" =~ ^[[:space:]]*$ ]] && continue
    if [[ "$body" =~ ^[[:space:]]*(///|//!) ]]; then
      ((has_fence)) && need_ci "$f changes doc comments in a file with doctests"
      continue
    fi
    # Plain `//` and `/*` starts only; a leading `*` may be a deref (`*ptr = 1;`).
    [[ "$body" =~ ^[[:space:]]*(//|/\*) ]] && continue
    need_ci "$f changes a code line: ${body:0:80}"
  done < <(git diff -U0 --no-color --no-renames "$base" "$target" -- "$f" |
    grep -E '^[+-]' | grep -vE '^(\+\+\+|---) ')
done

echo "CI skippable: ${#changed[@]} file(s) changed, docs or comments only since ${base:0:7}"
