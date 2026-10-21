#!/usr/bin/env bash
# Quality gate. Run before claiming a task done and before every commit.
#   scripts/gate.sh          full gate: repo checks + fmt + clippy + tests
#   scripts/gate.sh --fast   repo checks + fmt only (used by the pre-commit hook)
source "$(dirname "$0")/lib.sh"

mode="full"
[[ "${1:-}" == "--fast" ]] && mode="fast"

failed=()
run_step() {
  local name="$1"
  shift
  echo "== $name"
  "$@" || failed+=("$name")
}

run_step "check-text" bash scripts/check-text.sh
run_step "check-lines" bash scripts/check-lines.sh
run_step "check-links" bash scripts/check-links.sh
run_step "check-docs" bash scripts/check-docs.sh

if [[ -f Cargo.toml ]]; then
  run_step "cargo fmt" cargo fmt --all -- --check
  if [[ "$mode" == "full" ]]; then
    run_step "cargo clippy" cargo clippy --workspace --all-targets -- -D warnings
    run_step "cargo test" cargo test --workspace
  fi
else
  echo "== cargo steps skipped (no Cargo.toml yet)"
fi

if ((${#failed[@]} > 0)); then
  echo "${C_RED}GATE FAILED${C_RESET}: ${failed[*]}"
  exit 1
fi
echo "${C_GREEN}GATE PASSED${C_RESET} ($mode)"
