#!/usr/bin/env bash
# Builds the workspace.
#   scripts/build.sh                 debug build of the whole workspace
#   scripts/build.sh release         release build, copies app binaries to dist/
#   scripts/build.sh debug -p NAME   build one package (e.g. mixlink-net)
source "$(dirname "$0")/lib.sh"

profile="${1:-debug}"
shift || true

if [[ ! -f Cargo.toml ]]; then
  echo "No Cargo.toml yet — create the workspace first (spec 0001)."
  exit 1
fi

case "$profile" in
  debug) cargo build --workspace "$@" ;;
  release) cargo build --workspace --release "$@" ;;
  *)
    echo "usage: scripts/build.sh [debug|release] [cargo args...]"
    exit 2
    ;;
esac || exit 1

if [[ "$profile" == "release" ]]; then
  mkdir -p dist
  for bin in mixlink-host mixlink-client; do
    for ext in ".exe" ""; do
      src="target/release/$bin$ext"
      if [[ -f "$src" ]]; then
        cp "$src" dist/
        ok "dist/$bin$ext"
        break
      fi
    done
  done
fi
