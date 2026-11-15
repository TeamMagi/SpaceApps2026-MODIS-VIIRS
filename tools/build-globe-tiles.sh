#!/usr/bin/env bash
# Cuts the NASA Blue Marble (public domain) equirectangular mosaic into the globe's imagery:
# web/public/globe/earth.jpg (4096x2048 base for far views) and web/public/globe/tiles/{row}_{col}.jpg
# (16 x 8 tiles of 22.5 deg, 1024x1024 px each; row 0 starts at 90N, col 0 at 180W).
#
# Usage:
#   tools/build-globe-tiles.sh [SOURCE_JPG]
#
#   SOURCE_JPG  21600x10800 Blue Marble mosaic (lon -180..180, lat 90..-90). When omitted it is
#               downloaded with curl from NASA into a temporary directory and removed afterwards.
#
# Environment:
#   PYTHON      Python interpreter with Pillow installed (default: python3, then python).

set -euo pipefail

SOURCE_URL="https://assets.science.nasa.gov/content/dam/science/esd/eo/images/bmng/bmng-topography-bathymetry/july/world.topo.bathy.200407.3x21600x10800.jpg"

usage() {
  sed -n '2,13p' "$0" | sed 's/^# \{0,1\}//'
}

die() {
  echo "build-globe-tiles: $*" >&2
  exit 1
}

if [ "$#" -gt 1 ] || [ "${1:-}" = "-h" ] || [ "${1:-}" = "--help" ]; then
  usage
  [ "$#" -gt 1 ] && exit 2
  exit 0
fi

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT_DIR="$ROOT/web/public/globe"
[ -d "$OUT_DIR" ] || die "output directory not found: $OUT_DIR"

PY="${PYTHON:-}"
if [ -z "$PY" ]; then
  if command -v python3 >/dev/null 2>&1 && python3 -c "import sys" >/dev/null 2>&1; then
    PY=python3
  elif command -v python >/dev/null 2>&1; then
    PY=python
  else
    die "Python not found; set PYTHON to an interpreter with Pillow installed"
  fi
fi
"$PY" -I -c "import PIL" >/dev/null 2>&1 || die "Pillow is not installed for $PY (pip install Pillow)"

TMP_DIR=""
cleanup() {
  if [ -n "$TMP_DIR" ]; then rm -rf "$TMP_DIR"; fi
}
trap cleanup EXIT

if [ "$#" -eq 1 ]; then
  SOURCE="$1"
  [ -f "$SOURCE" ] || die "source image not found: $SOURCE"
else
  command -v curl >/dev/null 2>&1 || die "curl not found; pass the source image path instead"
  TMP_DIR="$(mktemp -d)"
  SOURCE="$TMP_DIR/bmng.jpg"
  echo "Downloading Blue Marble mosaic (~30 MB)..."
  curl --fail --location --silent --show-error --output "$SOURCE" "$SOURCE_URL" \
    || die "download failed: $SOURCE_URL"
fi

"$PY" -I - "$SOURCE" "$OUT_DIR" <<'PY'
import os
import sys

from PIL import Image

Image.MAX_IMAGE_PIXELS = None

COLS, ROWS = 16, 8
TILE_PX = 1024
BASE_SIZE = (4096, 2048)
QUALITY = 80

source, out_dir = sys.argv[1], sys.argv[2]
tiles_dir = os.path.join(out_dir, "tiles")
os.makedirs(tiles_dir, exist_ok=True)

with Image.open(source) as img:
    img = img.convert("RGB")
    width, height = img.size
    if width != 2 * height or width % COLS or height % ROWS:
        sys.exit(f"build-globe-tiles: expected a 2:1 equirectangular image divisible into "
                 f"{COLS}x{ROWS} tiles, got {width}x{height}")

    def save(image, path, quality):
        image.save(path, "JPEG", quality=quality, optimize=True, progressive=False)
        return os.path.getsize(path)

    total = save(img.resize(BASE_SIZE, Image.LANCZOS, reducing_gap=3.0),
                 os.path.join(out_dir, "earth.jpg"), 85)
    print(f"earth.jpg {BASE_SIZE[0]}x{BASE_SIZE[1]}: {total / 1e6:.2f} MB")

    step_x, step_y = width // COLS, height // ROWS
    tiles_bytes = 0
    for row in range(ROWS):
        for col in range(COLS):
            box = (col * step_x, row * step_y, (col + 1) * step_x, (row + 1) * step_y)
            tile = img.crop(box).resize((TILE_PX, TILE_PX), Image.LANCZOS)
            tiles_bytes += save(tile, os.path.join(tiles_dir, f"{row}_{col}.jpg"), QUALITY)
        print(f"row {row + 1}/{ROWS} done")
    print(f"{ROWS * COLS} tiles {TILE_PX}x{TILE_PX}: {tiles_bytes / 1e6:.2f} MB")
PY

echo "Wrote $OUT_DIR/earth.jpg and $OUT_DIR/tiles/"
