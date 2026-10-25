#!/usr/bin/env bash
# Makes test bitstreams for `mixlink-stream --play` (spec 0013) with ffmpeg.exe and a hardware
# encoder (the LGPL FFmpeg build has no x264). Output: out/test-<size>-<fps>.h264 and .hevc (BT.709),
# plus out/test-<size>-<fps>-<codec>.mkv (same bitstream, for ffplay side by side).
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

# BT.709 limited range, written into the stream, as the host encoder sends it (the stream window
# decodes BT.709). Without it FFmpeg converts RGB with BT.601 and tags nothing.
color=(-vf "scale=out_color_matrix=bt709:out_range=tv,format=nv12"
  -colorspace bt709 -color_primaries bt709 -color_trc bt709 -color_range tv)

mkdir -p out
frames=$((fps * seconds))
for codec in h264 hevc; do
  file="out/test-$size-$fps.$codec"
  ffmpeg -hide_banner -loglevel error -y -f lavfi -i "testsrc2=size=$size:rate=$fps" \
    -frames:v "$frames" "${color[@]}" -c:v "${codec}_$encoder" "${opts[@]}" -bf 0 -g "$fps" "$file" ||
    { err "$file: encode failed"; continue; }
  # Same bitstream in MKV with timestamps: ffplay cannot loop the raw file (side-by-side checks).
  ffmpeg -hide_banner -loglevel error -y -framerate "$fps" -i "$file" -c copy "${file%.*}-$codec.mkv" ||
    err "$file: MKV copy failed"
  counted="$(ffprobe -v error -count_frames -select_streams v:0 \
    -show_entries stream=nb_read_frames -of csv=p=0 "$file" | tr -d '\r')"
  if [[ "$counted" == "$frames" ]]; then
    ok "$file ($frames frames)"
  else
    err "$file: ffprobe counted $counted frames, expected $frames"
  fi
done
finish "make-test-stream"
