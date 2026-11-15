#!/usr/bin/env bash
# End-to-end smoke tests against a running PyroSync server.
# Usage: tools/smoke_test.sh [BASE_URL]

set -euo pipefail
export LC_ALL=C

DEFAULT_URL="http://localhost:3001"
EXPECT_LIVE="${SMOKE_EXPECT_LIVE:-}"

usage() {
    cat <<EOF
Usage: $0 [BASE_URL]

Run end-to-end smoke checks against a running PyroSync server.
Uses only bash, curl, and standard POSIX text tools (grep, sed, awk).

Arguments:
  BASE_URL           Target server URL (default: $DEFAULT_URL)

Environment:
  SMOKE_EXPECT_LIVE  Live FIRMS expectation for /api/harmonize:
                       1     Require HTTP 200 (FIRMS_API_KEY configured)
                       0     Require HTTP 503 (no FIRMS_API_KEY)
                       unset Accept either 200 or 503 with note (default)
EOF
}

if [ "${1:-}" = "-h" ] || [ "${1:-}" = "--help" ]; then
    usage
    exit 0
fi

if [ "$#" -gt 1 ]; then
    echo "Error: expected at most 1 argument (BASE_URL), got $#." >&2
    usage >&2
    exit 2
fi

case "$EXPECT_LIVE" in
    ""|"0"|"1") ;;
    *)
        echo "Error: invalid SMOKE_EXPECT_LIVE='$EXPECT_LIVE' (expected 1, 0, or unset)." >&2
        exit 2
        ;;
esac

if ! command -v curl >/dev/null 2>&1; then
    echo "Error: 'curl' is required but was not found in PATH." >&2
    exit 1
fi

BASE_URL="${1:-$DEFAULT_URL}"
BASE_URL="${BASE_URL%/}"

TMP_DIR=$(mktemp -d)
HDR_FILE="$TMP_DIR/headers.txt"
BODY_FILE="$TMP_DIR/body.txt"
ERR_FILE="$TMP_DIR/curl.err"
trap 'rm -rf "$TMP_DIR"' EXIT

PASS_COUNT=0
FAIL_COUNT=0
SKIP_COUNT=0

pass() {
    PASS_COUNT=$((PASS_COUNT + 1))
    printf 'PASS  %s\n' "$1"
}

fail() {
    FAIL_COUNT=$((FAIL_COUNT + 1))
    printf 'FAIL  %s\n' "$1"
}

skip() {
    SKIP_COUNT=$((SKIP_COUNT + 1))
    printf 'SKIP  %s\n' "$1"
}

REQ_STATUS=""
REQ_TIME=""
REQ_ERR=""

http_get() {
    local path="$1"
    local timeout="${2:-30}"
    local url="${BASE_URL}${path}"
    : > "$HDR_FILE"
    : > "$BODY_FILE"
    : > "$ERR_FILE"
    REQ_STATUS=""
    REQ_TIME=""
    REQ_ERR=""

    local meta=""
    if meta=$(curl -sS --max-time "$timeout" -D "$HDR_FILE" -o "$BODY_FILE" -w '%{http_code} %{time_total}' "$url" 2>"$ERR_FILE"); then
        REQ_STATUS="${meta%% *}"
        REQ_TIME="${meta#* }"
        return 0
    fi
    REQ_ERR=$(tr -d '\r\n' < "$ERR_FILE")
    return 1
}

header_value() {
    local name="$1"
    tr -d '\r' < "$HDR_FILE" | awk -v key="$(printf '%s' "$name" | tr '[:upper:]' '[:lower:]')" '
        BEGIN { found = 0 }
        !found {
            idx = index($0, ":")
            if (idx > 0) {
                k = tolower(substr($0, 1, idx - 1))
                if (k == key) {
                    v = substr($0, idx + 1)
                    sub(/^[ \t]+/, "", v)
                    sub(/[ \t]+$/, "", v)
                    print v
                    found = 1
                }
            }
        }
    '
}

count_matches() {
    local pattern="$1"
    { grep -o "$pattern" "$BODY_FILE" || true; } | wc -l | tr -d ' '
}

is_json_ct() {
    local ct
    ct=$(header_value "content-type" | tr '[:upper:]' '[:lower:]')
    case "$ct" in
        *application/json*) return 0 ;;
        *) return 1 ;;
    esac
}

time_under() {
    local limit="$1"
    awk -v t="$REQ_TIME" -v lim="$limit" 'BEGIN { exit !(t < lim) }'
}

# 1. GET /api/health -> 200
if ! http_get "/api/health" 30; then
    fail "1. GET /api/health -> request error: $REQ_ERR"
elif [ "$REQ_STATUS" != "200" ]; then
    fail "1. GET /api/health -> expected 200, got $REQ_STATUS"
else
    pass "1. GET /api/health -> 200 (${REQ_TIME}s)"
fi

# 2. GET / -> 200 and HTML (SKIP if frontend static assets are absent)
if ! http_get "/" 30; then
    fail "2. GET / -> request error: $REQ_ERR"
elif [ "$REQ_STATUS" = "404" ]; then
    skip "2. GET / -> 404 (frontend static assets not present)"
elif [ "$REQ_STATUS" != "200" ]; then
    fail "2. GET / -> expected 200 or 404, got $REQ_STATUS"
else
    ct=$(header_value "content-type" | tr '[:upper:]' '[:lower:]')
    if echo "$ct" | grep -q "text/html" && grep -qiE '<!doctype html|<html' "$BODY_FILE"; then
        pass "2. GET / -> 200 HTML"
    else
        fail "2. GET / -> 200 but response is not HTML (content-type: ${ct:-none})"
    fi
fi

# 3. GET /api/regions -> 200, 238 regions, 0 too_large, 238 precomputed (SKIP if archive missing)
if ! http_get "/api/regions" 30; then
    fail "3. GET /api/regions -> request error: $REQ_ERR"
elif [ "$REQ_STATUS" = "503" ]; then
    skip "3. GET /api/regions -> 503 (archive missing; run 'make data')"
elif [ "$REQ_STATUS" != "200" ]; then
    fail "3. GET /api/regions -> expected 200, got $REQ_STATUS"
else
    region_count=$(count_matches '"precomputed":')
    if [ "$region_count" -eq 0 ]; then
        skip "3. GET /api/regions -> 0 regions (archive empty; run 'make data')"
    else
        too_large_count=$(count_matches '"too_large": *true')
        precomputed_count=$(count_matches '"precomputed": *true')
        if [ "$region_count" -eq 238 ] && [ "$too_large_count" -eq 0 ] && [ "$precomputed_count" -eq 238 ]; then
            pass "3. GET /api/regions -> 200 (238 regions, 0 too_large, 238 precomputed)"
        else
            fail "3. GET /api/regions -> expected 238/0/238, got regions=$region_count too_large=$too_large_count precomputed=$precomputed_count"
        fi
    fi
fi

# 4. GET /api/series?region=Cyprus -> 200 + X-Series-Source header
if ! http_get "/api/series?region=Cyprus" 30; then
    fail "4. GET /api/series?region=Cyprus -> request error: $REQ_ERR"
elif [ "$REQ_STATUS" != "200" ]; then
    fail "4. GET /api/series?region=Cyprus -> expected 200, got $REQ_STATUS"
else
    series_src=$(header_value "x-series-source")
    if [ -n "$series_src" ]; then
        pass "4. GET /api/series?region=Cyprus -> 200 (X-Series-Source: $series_src)"
    else
        fail "4. GET /api/series?region=Cyprus -> 200 but missing X-Series-Source header"
    fi
fi

# 5. GET /api/series?region=Brazil -> 200, <10s, X-Series-Source in {bundled,disk,memory}
if ! http_get "/api/series?region=Brazil" 15; then
    fail "5. GET /api/series?region=Brazil -> request error: $REQ_ERR"
elif [ "$REQ_STATUS" != "200" ]; then
    fail "5. GET /api/series?region=Brazil -> expected 200, got $REQ_STATUS"
else
    series_src=$(header_value "x-series-source")
    case "$series_src" in
        bundled|disk|memory) src_ok=1 ;;
        *) src_ok=0 ;;
    esac
    if [ "$src_ok" -eq 1 ] && time_under 10; then
        pass "5. GET /api/series?region=Brazil -> 200 in ${REQ_TIME}s (X-Series-Source: $series_src)"
    else
        fail "5. GET /api/series?region=Brazil -> time=${REQ_TIME}s (<10s?), X-Series-Source='${series_src:-missing}'"
    fi
fi

# 6. GET /api/calendar?country=Cyprus&from=2019&to=2019 -> 200 + required keys
if ! http_get "/api/calendar?country=Cyprus&from=2019&to=2019" 30; then
    fail "6. GET /api/calendar (Cyprus 2019) -> request error: $REQ_ERR"
elif [ "$REQ_STATUS" != "200" ]; then
    fail "6. GET /api/calendar (Cyprus 2019) -> expected 200, got $REQ_STATUS"
elif grep -q '"uncertainty"' "$BODY_FILE" && grep -q '"total_frp"' "$BODY_FILE" && grep -q '"frp_per_detection_eq"' "$BODY_FILE"; then
    pass "6. GET /api/calendar (Cyprus 2019) -> 200 with uncertainty, total_frp, frp_per_detection_eq"
else
    fail "6. GET /api/calendar (Cyprus 2019) -> 200 but missing expected keys"
fi

# 7. GET /api/points?region=Cyprus&year=2021&month=8 -> 200 + "stages" (allow up to 120s on cold archive coverage scan)
if ! http_get "/api/points?region=Cyprus&year=2021&month=8" 120; then
    fail "7. GET /api/points (Cyprus 2021-08) -> request error: $REQ_ERR"
elif [ "$REQ_STATUS" != "200" ]; then
    fail "7. GET /api/points (Cyprus 2021-08) -> expected 200, got $REQ_STATUS"
elif grep -q '"stages"' "$BODY_FILE"; then
    pass "7. GET /api/points (Cyprus 2021-08) -> 200 with stages (${REQ_TIME}s)"
else
    fail "7. GET /api/points (Cyprus 2021-08) -> 200 but missing 'stages'"
fi

# 8. GET /api/points?region=Cyprus&year=2021&limit=0 -> 400 JSON ("error")
if ! http_get "/api/points?region=Cyprus&year=2021&limit=0" 30; then
    fail "8. GET /api/points (limit=0) -> request error: $REQ_ERR"
elif [ "$REQ_STATUS" = "400" ] && is_json_ct && grep -q '"error"' "$BODY_FILE"; then
    pass "8. GET /api/points (limit=0) -> 400 JSON error"
else
    fail "8. GET /api/points (limit=0) -> expected 400 JSON error, got $REQ_STATUS"
fi

# 9. GET /api/points?region=Malta&year=2021&limit=1 -> <5s (infinite loop regression)
if ! http_get "/api/points?region=Malta&year=2021&limit=1" 10; then
    fail "9. GET /api/points (Malta 2021 limit=1) -> request error: $REQ_ERR"
elif [ "$REQ_STATUS" != "200" ]; then
    fail "9. GET /api/points (Malta 2021 limit=1) -> expected 200, got $REQ_STATUS"
elif time_under 5; then
    pass "9. GET /api/points (Malta 2021 limit=1) -> 200 in ${REQ_TIME}s (<5s)"
else
    fail "9. GET /api/points (Malta 2021 limit=1) -> 200 but took ${REQ_TIME}s (>=5s)"
fi

# 10. GET /api/calendar?country=Malta&from=abc -> 400 JSON ("error")
if ! http_get "/api/calendar?country=Malta&from=abc" 30; then
    fail "10. GET /api/calendar (from=abc) -> request error: $REQ_ERR"
elif [ "$REQ_STATUS" = "400" ] && is_json_ct && grep -q '"error"' "$BODY_FILE"; then
    pass "10. GET /api/calendar (from=abc) -> 400 JSON error"
else
    fail "10. GET /api/calendar (from=abc) -> expected 400 JSON error, got $REQ_STATUS"
fi

# 11. GET /api/does-not-exist -> 404 JSON
if ! http_get "/api/does-not-exist" 30; then
    fail "11. GET /api/does-not-exist -> request error: $REQ_ERR"
elif [ "$REQ_STATUS" = "404" ] && is_json_ct && grep -q '"error"' "$BODY_FILE"; then
    pass "11. GET /api/does-not-exist -> 404 JSON error"
else
    fail "11. GET /api/does-not-exist -> expected 404 JSON error, got $REQ_STATUS"
fi

# 12. GET /api/calendar?country=Atlantis&from=2019&to=2019 -> 404 JSON
if ! http_get "/api/calendar?country=Atlantis&from=2019&to=2019" 30; then
    fail "12. GET /api/calendar (Atlantis) -> request error: $REQ_ERR"
elif [ "$REQ_STATUS" = "404" ] && is_json_ct && grep -q '"error"' "$BODY_FILE"; then
    pass "12. GET /api/calendar (Atlantis) -> 404 JSON error"
else
    fail "12. GET /api/calendar (Atlantis) -> expected 404 JSON error, got $REQ_STATUS"
fi

# 13. country 65 chars -> 400
COUNTRY_65="aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
if ! http_get "/api/calendar?country=${COUNTRY_65}&from=2019&to=2019" 30; then
    fail "13. GET /api/calendar (65-char country) -> request error: $REQ_ERR"
elif [ "$REQ_STATUS" = "400" ] && is_json_ct && grep -q '"error"' "$BODY_FILE"; then
    pass "13. GET /api/calendar (65-char country) -> 400 JSON error"
else
    fail "13. GET /api/calendar (65-char country) -> expected 400 JSON error, got $REQ_STATUS"
fi

# 14. GET /api/harmonize?west=26&south=36&east=45&north=42&days=1 -> 200 or 503
if ! http_get "/api/harmonize?west=26&south=36&east=45&north=42&days=1" 30; then
    fail "14. GET /api/harmonize (live check) -> request error: $REQ_ERR"
else
    case "$EXPECT_LIVE" in
        1)
            if [ "$REQ_STATUS" = "200" ]; then
                pass "14. GET /api/harmonize -> 200 (SMOKE_EXPECT_LIVE=1)"
            else
                fail "14. GET /api/harmonize -> expected 200 (SMOKE_EXPECT_LIVE=1), got $REQ_STATUS"
            fi
            ;;
        0)
            if [ "$REQ_STATUS" = "503" ]; then
                pass "14. GET /api/harmonize -> 503 (no FIRMS key, SMOKE_EXPECT_LIVE=0)"
            else
                fail "14. GET /api/harmonize -> expected 503 (SMOKE_EXPECT_LIVE=0), got $REQ_STATUS"
            fi
            ;;
        "")
            if [ "$REQ_STATUS" = "200" ]; then
                pass "14. GET /api/harmonize -> 200 (FIRMS key active, ${REQ_TIME}s)"
            elif [ "$REQ_STATUS" = "503" ]; then
                pass "14. GET /api/harmonize -> 503 (no FIRMS key configured)"
            else
                fail "14. GET /api/harmonize -> expected 200 or 503, got $REQ_STATUS"
            fi
            ;;
    esac
fi

echo ""
printf '%d passed, %d failed, %d skipped\n' "$PASS_COUNT" "$FAIL_COUNT" "$SKIP_COUNT"

if [ "$FAIL_COUNT" -gt 0 ]; then
    exit 1
fi
