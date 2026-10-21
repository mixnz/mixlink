#!/usr/bin/env bash
# Shared helpers for scripts/*.sh. Source it, do not execute it.

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT" || exit 1

ERRORS=0
WARNINGS=0

if [[ -t 1 ]]; then
  C_RED=$'\e[31m' C_YELLOW=$'\e[33m' C_GREEN=$'\e[32m' C_RESET=$'\e[0m'
else
  C_RED="" C_YELLOW="" C_GREEN="" C_RESET=""
fi

err() { echo "${C_RED}ERROR${C_RESET} $*"; ERRORS=$((ERRORS + 1)); }
warn() { echo "${C_YELLOW}WARN${C_RESET}  $*"; WARNINGS=$((WARNINGS + 1)); }
ok() { echo "${C_GREEN}OK${C_RESET}    $*"; }

# Lists tracked + untracked-but-not-ignored files, one per line.
repo_files() {
  git ls-files -co --exclude-standard | while IFS= read -r f; do
    [[ -f "$f" ]] && printf '%s\n' "$f"
  done
}

# Prints a summary line and exits non-zero when errors were reported.
finish() {
  local name="$1"
  if ((ERRORS > 0)); then
    echo "${C_RED}FAIL${C_RESET}  $name: $ERRORS error(s), $WARNINGS warning(s)"
    exit 1
  fi
  ok "$name ($WARNINGS warning(s))"
}
