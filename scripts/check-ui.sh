#!/usr/bin/env bash
# Frontend checks for the npm workspace (apps/*/ui, packages/*): lint, typecheck, test, build.
# Runs before cargo in the gate: Tauri embeds apps/<app>/ui/dist at compile time.
source "$(dirname "$0")/lib.sh"

if ! command -v npm >/dev/null 2>&1; then
  err "npm not found on PATH — install Node (version in .nvmrc)"
  finish "check-ui"
fi

# Reinstall only when the lockfile changed since the last install.
if [[ ! -f node_modules/.package-lock.json || package-lock.json -nt node_modules/.package-lock.json ]]; then
  npm ci --no-audit --no-fund || err "npm ci failed"
fi

if ((ERRORS == 0)); then
  for step in lint typecheck test build; do
    npm run --silent "$step" || err "npm run $step failed"
  done
fi

finish "check-ui"
