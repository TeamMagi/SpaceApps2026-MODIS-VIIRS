#!/usr/bin/env bash
# Measure the persistent hot-cell (gas flare) mask threshold and the SAMA night filter on the
# local FIRMS country archive (read-only), or print how many rows each filter removes.
# Usage: tools/static-mask-check.sh <mask|sama|pipeline> <from-year> <to-year> <Country>...

set -euo pipefail

usage() {
    cat <<EOF
Usage: $0 <mask|sama|pipeline> <from-year> <to-year> <Country>...

Modes:
  mask      Per month threshold: share of type-0 rows a per-year H3 r7 mask drops and the share
            of those inside cells FIRMS flags as static (type 1/2)
  sama      Isolation rate of night detections inside vs outside the SAMA box, per sensor family
  pipeline  Rows removed by static_mask (default config) and by sama (when enabled), per year

Examples:
  $0 mask 2013 2024 Nigeria Iraq Algeria Portugal
  $0 sama 2018 2020 Brazil Angola Mexico

Environment:
  ARCHIVE_CACHE_DIR  Local archive directory (default: .cache/firms-archive)
EOF
}

if [ "$#" -lt 4 ]; then
    usage >&2
    exit 1
fi

case "$1" in
    mask|sama|pipeline) ;;
    -h|--help) usage; exit 0 ;;
    *) echo "error: unknown mode '$1'" >&2; usage >&2; exit 1 ;;
esac

if [ ! -d "${ARCHIVE_CACHE_DIR:-.cache/firms-archive}" ]; then
    echo "error: archive directory '${ARCHIVE_CACHE_DIR:-.cache/firms-archive}' not found (run make data)" >&2
    exit 1
fi

exec cargo run --release --quiet -p pyrosync-firms --example static_mask_check -- "$@"
