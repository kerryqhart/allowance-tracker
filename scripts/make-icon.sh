#!/usr/bin/env bash
#
# Regenerate the macOS app icons from the full-bleed master art.
#
# Why this exists
# ---------------
# egui/eframe sets the *running* app's Dock tile at runtime from the raw
# RGBA of assets/app-icon.png (NSApplication.setApplicationIconImage). It
# hands those pixels to macOS verbatim: no transparent margin, no rounded
# mask. macOS draws a Dock tile edge-to-edge, so a full-bleed square looks
# oversized next to other apps. eframe also does nothing for the .app's
# Finder icon — that comes solely from a bundled .icns + CFBundleIconFile,
# which cargo-bundle 0.9 does not generate.
#
# This script fixes both from one master:
#   * assets/app-icon.png  -> 1024x1024 with the standard ~10% transparent
#                             margin baked in (the runtime Dock icon).
#   * assets/AppIcon.icns   -> multi-resolution icns for the .app's Finder
#                             icon (embedded by scripts/install.sh).
#
# The art already has rounded corners, so we only add the margin (center
# the art on a larger transparent canvas) — we do not re-round it.
#
# Requires: ImageMagick (magick); sips + iconutil (bundled with macOS).

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

MASTER="$REPO_ROOT/icon.png"                             # full-bleed master art
OUT_PNG="$REPO_ROOT/egui-frontend/assets/app-icon.png"  # runtime Dock icon
OUT_ICNS="$REPO_ROOT/egui-frontend/assets/AppIcon.icns" # bundle Finder icon

CANVAS=1024   # full icon canvas (px)
ART=824       # art size within the canvas; matches Apple's rounded-rect grid
              # (~100px transparent margin per side)

echo "==> Master art: $MASTER"
if [ ! -f "$MASTER" ]; then
  echo "error: master art not found: $MASTER" >&2
  exit 1
fi

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

echo "==> Building padded ${CANVAS}x${CANVAS} PNG (art ${ART}px, centered, transparent margin)"
magick "$MASTER" -resize "${ART}x${ART}" -background none -gravity center \
  -extent "${CANVAS}x${CANVAS}" "$OUT_PNG"

echo "==> Building AppIcon.icns from the padded PNG"
ICONSET="$TMP/AppIcon.iconset"
mkdir -p "$ICONSET"
sips -z 16   16   "$OUT_PNG" --out "$ICONSET/icon_16x16.png"      >/dev/null
sips -z 32   32   "$OUT_PNG" --out "$ICONSET/icon_16x16@2x.png"   >/dev/null
sips -z 32   32   "$OUT_PNG" --out "$ICONSET/icon_32x32.png"      >/dev/null
sips -z 64   64   "$OUT_PNG" --out "$ICONSET/icon_32x32@2x.png"   >/dev/null
sips -z 128  128  "$OUT_PNG" --out "$ICONSET/icon_128x128.png"    >/dev/null
sips -z 256  256  "$OUT_PNG" --out "$ICONSET/icon_128x128@2x.png" >/dev/null
sips -z 256  256  "$OUT_PNG" --out "$ICONSET/icon_256x256.png"    >/dev/null
sips -z 512  512  "$OUT_PNG" --out "$ICONSET/icon_256x256@2x.png" >/dev/null
sips -z 512  512  "$OUT_PNG" --out "$ICONSET/icon_512x512.png"    >/dev/null
sips -z 1024 1024 "$OUT_PNG" --out "$ICONSET/icon_512x512@2x.png" >/dev/null
iconutil -c icns "$ICONSET" -o "$OUT_ICNS"

echo "==> Done."
echo "    Runtime Dock icon: $OUT_PNG"
echo "    Bundle icon:       $OUT_ICNS"
