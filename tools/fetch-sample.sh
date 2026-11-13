#!/usr/bin/env bash
# Download sample FIRMS CSV data for local development.
# Usage: ./tools/fetch-sample.sh [API_KEY]

set -euo pipefail

API_KEY="${1:-${FIRMS_API_KEY:-}}"
if [ -z "$API_KEY" ]; then
  echo "Usage: $0 <FIRMS_API_KEY>"
  echo "Or set FIRMS_API_KEY environment variable"
  exit 1
fi

BASE="https://firms.modaps.eosdis.nasa.gov/api/area/csv"
BBOX="25,35,45,42"  # Turkey bounding box
DAYS=2

mkdir -p data/

echo "Fetching MODIS data..."
curl -sS "${BASE}/${API_KEY}/MODIS_NRT/${BBOX}/${DAYS}" -o data/modis_sample.csv

echo "Fetching VIIRS SNPP data..."
curl -sS "${BASE}/${API_KEY}/VIIRS_SNPP_NRT/${BBOX}/${DAYS}" -o data/viirs_snpp_sample.csv

echo "Fetching VIIRS NOAA-20 data..."
curl -sS "${BASE}/${API_KEY}/VIIRS_NOAA20_NRT/${BBOX}/${DAYS}" -o data/viirs_noaa20_sample.csv

echo "Done. Files saved in data/"
ls -lh data/*.csv 2>/dev/null || echo "No files downloaded (API key may be invalid)"
