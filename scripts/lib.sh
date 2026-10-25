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

# FFmpeg 8.1 LGPL shared (BtbN) + libclang for `ffmpeg-sys-next` (spec 0013). Defaults match
# .cargo/config.toml; puts the FFmpeg DLLs on PATH for tests and runs. Returns 1 when missing.
ffmpeg_env() {
  export FFMPEG_DIR="${FFMPEG_DIR:-C:\\tools\\ffmpeg}"
  export LIBCLANG_PATH="${LIBCLANG_PATH:-C:\\Program Files\\LLVM\\bin}"
  local ff_bin libclang
  ff_bin="$(cygpath -u "$FFMPEG_DIR")/bin"
  libclang="$(cygpath -u "$LIBCLANG_PATH")/libclang.dll"
  if [[ ! -f "$ff_bin/avcodec-62.dll" ]]; then
    err "FFmpeg not found in $FFMPEG_DIR: unpack BtbN ffmpeg-n8.1-latest-win64-lgpl-shared-8.1.zip there or set FFMPEG_DIR"
    return 1
  fi
  if [[ ! -f "$libclang" ]]; then
    err "libclang.dll not found in $LIBCLANG_PATH: install LLVM or set LIBCLANG_PATH"
    return 1
  fi
  export PATH="$ff_bin:$PATH"
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
