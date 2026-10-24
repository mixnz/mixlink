#!/usr/bin/env bash
# Quality gate. Run before claiming a task done and before every push.
#   scripts/gate.sh               full gate: repo checks + UI + fmt + clippy + tests
#   scripts/gate.sh --fast        repo checks + fmt (pre-commit hook)
#   scripts/gate.sh --checks      repo checks only (CI `docs` job)
#   scripts/gate.sh --ui          UI checks only (npm lint/typecheck/test/build)
#   scripts/gate.sh --cargo lint  UI + fmt + clippy (CI `lint` job)
#   scripts/gate.sh --cargo test  UI + tests (CI `test` job)
# UI runs before cargo: Tauri embeds apps/<app>/ui/dist at compile time.
source "$(dirname "$0")/lib.sh"

checks=1 ui=1 fmt=1 clippy=1 tests=1
case "${1:-}" in
  "") ;;
  --fast) ui=0 clippy=0 tests=0 ;;
  --checks) ui=0 fmt=0 clippy=0 tests=0 ;;
  --ui) checks=0 fmt=0 clippy=0 tests=0 ;;
  --cargo)
    checks=0
    case "${2:-}" in
      lint) tests=0 ;;
      test) fmt=0 clippy=0 ;;
      *) echo "usage: gate.sh --cargo lint|test" >&2; exit 64 ;;
    esac
    ;;
  *) echo "usage: gate.sh [--fast | --checks | --ui | --cargo lint|test]" >&2; exit 64 ;;
esac

failed=()
run_step() {
  local name="$1"
  shift
  echo "== $name"
  "$@" || failed+=("$name")
}

if ((checks)); then
  run_step "check-text" bash scripts/check-text.sh
  run_step "check-lines" bash scripts/check-lines.sh
  run_step "check-links" bash scripts/check-links.sh
  run_step "check-docs" bash scripts/check-docs.sh
  run_step "check-deps" bash scripts/check-deps.sh
  run_step "check-refs" bash scripts/check-refs.sh
fi

((ui)) && run_step "check-ui" bash scripts/check-ui.sh

if ((fmt || clippy || tests)); then
  if [[ -f Cargo.toml ]]; then
    ((fmt)) && run_step "cargo fmt" cargo fmt --all -- --check
    ((clippy)) && run_step "cargo clippy" cargo clippy --workspace --all-targets -- -D warnings
    ((tests)) && run_step "cargo test" cargo test --workspace
  else
    echo "== cargo steps skipped (no Cargo.toml yet)"
  fi
fi

if ((${#failed[@]} > 0)); then
  echo "${C_RED}GATE FAILED${C_RESET}: ${failed[*]}"
  exit 1
fi
echo "${C_GREEN}GATE PASSED${C_RESET} (${1:-full}${2:+ $2})"
