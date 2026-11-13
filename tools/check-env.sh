#!/usr/bin/env bash
# Verify that required environment variables and tools are available.

set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
NC='\033[0m'

ok()   { printf "${GREEN}[OK]${NC}   %s\n" "$1"; }
fail() { printf "${RED}[FAIL]${NC} %s\n" "$1"; ERRORS=$((ERRORS+1)); }

ERRORS=0

command -v cargo  >/dev/null 2>&1 && ok "cargo found"  || fail "cargo not found"
command -v node   >/dev/null 2>&1 && ok "node found"   || fail "node not found"
command -v npm    >/dev/null 2>&1 && ok "npm found"    || fail "npm not found"
command -v docker >/dev/null 2>&1 && ok "docker found" || fail "docker not found (optional)"

if [ -f .env ]; then
  ok ".env file exists"
  if grep -q "FIRMS_API_KEY" .env; then
    ok "FIRMS_API_KEY is set"
  else
    fail "FIRMS_API_KEY not found in .env"
  fi
else
  fail ".env file not found (copy from .env.example)"
fi

echo ""
if [ "$ERRORS" -eq 0 ]; then
  echo "All checks passed."
else
  echo "$ERRORS check(s) failed."
  exit 1
fi
