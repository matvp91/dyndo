#!/usr/bin/env bash
# Configure the FFmpeg build shared by Linux and macOS release artifacts.
set -euo pipefail

if [[ $# -lt 1 || $# -gt 2 ]]; then
  echo "usage: $0 <install-prefix> [darwin-install-name-dir]" >&2
  exit 2
fi

configure_args=(
  --prefix="$1"
  --disable-static
  --enable-shared
  --disable-programs
  --disable-doc
  --disable-debug
  --disable-autodetect
  --disable-everything
  --enable-avcodec
  --enable-avdevice
  --enable-avfilter
  --enable-avformat
  --enable-avutil
  --enable-swresample
  --enable-swscale
  --enable-demuxer=mov
  "--enable-decoder=h264,hevc,av1"
  --enable-encoder=mjpeg
  "--enable-parser=h264,hevc,av1"
)

if [[ -n "${2:-}" ]]; then
  configure_args+=(--install-name-dir="$2")
fi

./configure "${configure_args[@]}"
