#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ANDROID_ICON_DIR="$ROOT/src-tauri/icons/android"
GENERATED_RES_DIR="$ROOT/src-tauri/gen/android/app/src/main/res"

LAUNCHER_SVG="$ANDROID_ICON_DIR/verbatim-icon-launcher.svg"
ROUND_SVG="$ANDROID_ICON_DIR/verbatim-icon-round.svg"
FOREGROUND_SVG="$ANDROID_ICON_DIR/verbatim-icon-foreground.svg"

RSVG_CONVERT="$(command -v rsvg-convert || true)"

if [[ -z "$RSVG_CONVERT" && -x /c/msys64/ucrt64/bin/rsvg-convert.exe ]]; then
  RSVG_CONVERT="/c/msys64/ucrt64/bin/rsvg-convert.exe"
fi

if [[ -z "$RSVG_CONVERT" && -x /mnt/c/msys64/ucrt64/bin/rsvg-convert.exe ]]; then
  RSVG_CONVERT="/mnt/c/msys64/ucrt64/bin/rsvg-convert.exe"
fi

if [[ -z "$RSVG_CONVERT" ]]; then
  echo "rsvg-convert is required. Install librsvg and retry." >&2
  exit 1
fi

render_png() {
  local src="$1"
  local out="$2"
  local size="$3"
  local src_arg="$src"
  local out_arg="$out"

  if [[ "$RSVG_CONVERT" == *.exe && "$src" == /mnt/* ]] && command -v wslpath >/dev/null 2>&1; then
    src_arg="$(wslpath -w "$src")"
    out_arg="$(wslpath -w "$out")"
  fi

  mkdir -p "$(dirname "$out")"
  "$RSVG_CONVERT" -w "$size" -h "$size" -o "$out_arg" "$src_arg"
}

render_density() {
  local density="$1"
  local launcher_size="$2"
  local foreground_size="$3"
  local base

  for base in "$ANDROID_ICON_DIR" "$GENERATED_RES_DIR"; do
    render_png "$LAUNCHER_SVG" "$base/mipmap-$density/ic_launcher.png" "$launcher_size"
    render_png "$ROUND_SVG" "$base/mipmap-$density/ic_launcher_round.png" "$launcher_size"
    render_png "$FOREGROUND_SVG" "$base/mipmap-$density/ic_launcher_foreground.png" "$foreground_size"
  done
}

render_density mdpi 48 108
render_density hdpi 72 162
render_density xhdpi 96 216
render_density xxhdpi 144 324
render_density xxxhdpi 192 432

echo "Regenerated Android launcher PNGs from $ANDROID_ICON_DIR"
