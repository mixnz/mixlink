#!/usr/bin/env bash
# Makes test bitstreams for `mixlink-stream --play` (spec 0013) with ffmpeg.exe and a hardware
# encoder (the LGPL FFmpeg build has no x264). Output: out/test-<size>-<fps>.h264 and .hevc.
#   scripts/make-test-stream.sh [--encoder nvenc|qsv] [--size 2560x1440] [--fps 120] [--seconds 10]
source "$(dirname "$0")/lib.sh"

encoder=nvenc size=2560x1440 fps=120 seconds=10
while (($# > 0)); do
  case "$1" in
    --encoder) encoder="$2"; shift 2 ;;
    --size) size="$2"; shift 2 ;;
    --fps) fps="$2"; shift 2 ;;
    --seconds) seconds="$2"; shift 2 ;;
    *) echo "usage: make-test-stream.sh [--encoder nvenc|qsv] [--size WxH] [--fps N] [--seconds N]" >&2; exit 64 ;;
  esac
done

ffmpeg_env || exit 1
case "$encoder" in
  nvenc) opts=(-preset p1 -tune ull -rc cbr -b:v 30M) ;;
  qsv) opts=(-preset veryfast -low_power 1 -b:v 30M) ;;
  *) err "unknown encoder $encoder (nvenc or qsv)"; exit 64 ;;
esac

mkdir -p out
frames=$((fps * seconds))
for codec in h264 hevc; do
  file="out/test-$size-$fps.$codec"
  ffmpeg -hide_banner -loglevel error -y -f lavfi -i "testsrc2=size=$size:rate=$fps" \
    -frames:v "$frames" -pix_fmt nv12 -c:v "${codec}_$encoder" "${opts[@]}" -bf 0 -g "$fps" "$file" ||
    { err "$file: encode failed"; continue; }
  counted="$(ffprobe -v error -count_frames -select_streams v:0 \
    -show_entries stream=nb_read_frames -of csv=p=0 "$file" | tr -d '\r')"
  if [[ "$counted" == "$frames" ]]; then
    ok "$file ($frames frames)"
  else
    err "$file: ffprobe counted $counted frames, expected $frames"
  fi
done
finish "make-test-stream"
