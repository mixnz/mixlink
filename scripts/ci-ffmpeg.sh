#!/usr/bin/env bash
# CI: makes FFmpeg (BtbN LGPL shared, pinned by FFMPEG_URL) available in FFMPEG_DIR for the
# `ffmpeg-sys-next` build (spec 0013). Skips the download when the cache restored it.
#   FFMPEG_URL=<zip url> FFMPEG_DIR=C:\ffmpeg scripts/ci-ffmpeg.sh
set -euo pipefail

: "${FFMPEG_URL:?set FFMPEG_URL}"
: "${FFMPEG_DIR:?set FFMPEG_DIR}"
dir="$(cygpath -u "$FFMPEG_DIR")"

if [[ -f "$dir/bin/avcodec-62.dll" ]]; then
  echo "FFmpeg already in $FFMPEG_DIR (cache)"
  exit 0
fi

tmp="$(mktemp -d)"
curl -fsSL --retry 3 -o "$tmp/ffmpeg.zip" "$FFMPEG_URL"
unzip -q "$tmp/ffmpeg.zip" -d "$tmp"
mkdir -p "$dir"
# The zip holds one top-level folder (bin/, include/, lib/).
cp -r "$tmp"/ffmpeg-*/. "$dir/"
rm -rf "$tmp"
ls "$dir/bin"
