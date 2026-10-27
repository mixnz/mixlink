#!/usr/bin/env bash
# Builds the workspace.
#   scripts/build.sh                 debug build of the whole workspace
#   scripts/build.sh release         UI + release build (UI embedded), copies the 4 app exes,
#                                    the FFmpeg DLLs and the unsigned driver packages to dist/
#   scripts/build.sh debug -p NAME   build one package (e.g. mixlink-net)
source "$(dirname "$0")/lib.sh"

profile="${1:-debug}"
RELEASE_FEATURES="mixlink-client/custom-protocol,mixlink-host/custom-protocol"
shift || true

# Copies one file into dist/; a file in use (running service or tray) is an error, not OK.
copy_out() {
  if cp "$1" "$2" 2>/dev/null; then
    ok "$3"
  else
    err "$3 not copied: file in use? Stop the MixLink service / tray, then copy it from $1"
  fi
}

if [[ ! -f Cargo.toml ]]; then
  echo "No Cargo.toml yet — create the workspace first (spec 0001)."
  exit 1
fi

# Tauri embeds apps/<app>/ui/dist at compile time.
if [[ "$profile" == "release" ]]; then
  bash scripts/check-ui.sh || exit 1
fi

case "$profile" in
  debug) cargo build --workspace "$@" ;;
  # custom-protocol: Tauri apps embed ui/dist instead of loading the Vite dev server.
  release) cargo build --workspace --release --features "$RELEASE_FEATURES" "$@" ;;
  *)
    echo "usage: scripts/build.sh [debug|release] [cargo args...]"
    exit 2
    ;;
esac || exit 1

if [[ "$profile" == "release" ]]; then
  mkdir -p dist
  for bin in mixlink-host mixlink-client mixlink-stream mixlink-svc; do
    for ext in ".exe" ""; do
      src="target/release/$bin$ext"
      if [[ -f "$src" ]]; then
        copy_out "$src" dist/ "dist/$bin$ext"
        break
      fi
    done
  done
  # FFmpeg DLLs for mixlink-stream's decoder (spec 0013); the .exe tools are not shipped.
  ffmpeg_env || exit 1
  for dll in "$(cygpath -u "$FFMPEG_DIR")"/bin/*.dll; do
    copy_out "$dll" dist/ "dist/$(basename "$dll")"
  done
  # Unsigned driver packages; `mixlink-svc drivers install` signs them per machine (spec 0014).
  powershell -NoProfile -ExecutionPolicy Bypass -File drivers/build.ps1 -Driver all || exit 1
  for name in vdisplay vhid; do
    rm -rf "dist/drivers/$name"
    mkdir -p "dist/drivers/$name"
    for f in "target/drivers/$name/pkg/"*; do
      copy_out "$f" "dist/drivers/$name/" "dist/drivers/$name/$(basename "$f")"
    done
  done
fi

if (( ERRORS > 0 )); then
  echo "BUILD FAILED: $ERRORS file(s) not copied to dist/"
  exit 1
fi
