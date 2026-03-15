#!/usr/bin/env bash
# deploy-pi-dev.sh — Set up the pi-mono dist symlink for local development.
#
# Replaces the dist/ directory in the globally-installed pi package with a
# symlink to pi-mono's built dist, so that `npm run build` in pi-mono is
# immediately reflected in the running pi binary (after restart).
#
# Run once after cloning. Idempotent — safe to re-run.
#
# Usage:
#   ./scripts/deploy-pi-dev.sh
#   ./scripts/deploy-pi-dev.sh --check   # report current state, no changes

set -euo pipefail

PI_INSTALL="/opt/homebrew/lib/node_modules/@styrene-lab/pi-coding-agent"
PI_MONO_DIST="$(cd "$(dirname "$0")/../.." && pwd)/pi-mono/packages/coding-agent/dist"
TARGET_DIST="${PI_INSTALL}/dist"

GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
DIM='\033[2m'
NC='\033[0m'

check_only=false
if [[ "${1:-}" == "--check" ]]; then
  check_only=true
fi

echo ""
echo "  pi-mono dev symlink setup"
echo "  ─────────────────────────"
echo ""
echo "  pi install:   ${PI_INSTALL}"
echo "  pi-mono dist: ${PI_MONO_DIST}"
echo ""

# Verify pi-mono dist exists and is built
if [[ ! -d "${PI_MONO_DIST}" ]]; then
  echo -e "  ${RED}✗${NC} pi-mono dist not found at ${PI_MONO_DIST}"
  echo -e "  ${DIM}  Run: cd ../../pi-mono && npm run build${NC}"
  echo ""
  exit 1
fi

if [[ ! -f "${PI_MONO_DIST}/cli.js" ]]; then
  echo -e "  ${YELLOW}⚠${NC} pi-mono dist exists but looks unbuilt (no cli.js)"
  echo -e "  ${DIM}  Run: cd ../../pi-mono && npm run build${NC}"
  echo ""
  exit 1
fi

# Check current state
if [[ -L "${TARGET_DIST}" ]]; then
  current_target=$(readlink "${TARGET_DIST}")
  if [[ "${current_target}" == "${PI_MONO_DIST}" ]]; then
    echo -e "  ${GREEN}✓${NC} Already linked to pi-mono dist"
    echo ""
    echo -e "  ${DIM}Dev loop: npm run build:pi (in omegon) → restart pi${NC}"
    echo ""
    exit 0
  else
    echo -e "  ${YELLOW}⚠${NC} Symlink exists but points elsewhere: ${current_target}"
    if [[ "${check_only}" == true ]]; then exit 1; fi
    echo -e "  ${DIM}  Relinking to pi-mono...${NC}"
    rm "${TARGET_DIST}"
  fi
elif [[ -d "${TARGET_DIST}" ]]; then
  if [[ "${check_only}" == true ]]; then
    echo -e "  ${YELLOW}⚠${NC} dist is a real directory (not yet symlinked)"
    exit 1
  fi
  echo -e "  Backing up existing dist → dist.bak"
  mv "${TARGET_DIST}" "${TARGET_DIST}.bak"
else
  echo -e "  ${RED}✗${NC} ${TARGET_DIST} does not exist — is pi installed?"
  exit 1
fi

# Create the symlink
ln -s "${PI_MONO_DIST}" "${TARGET_DIST}"
echo -e "  ${GREEN}✓${NC} Linked ${TARGET_DIST}"
echo -e "    → ${PI_MONO_DIST}"
echo ""
echo -e "  ${GREEN}Dev loop:${NC}"
echo -e "  ${DIM}  npm run build:pi   (in omegon, rebuilds pi-mono)${NC}"
echo -e "  ${DIM}  restart pi         (picks up changes immediately)${NC}"
echo ""
echo -e "  ${DIM}The backup of the original dist is at dist.bak${NC}"
echo ""
