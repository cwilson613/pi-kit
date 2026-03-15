#!/usr/bin/env bash
# Compatibility name retained for contributors, but this script now enforces the
# authoritative Omegon-owned lifecycle boundary: build, link, verify `omegon`,
# then stop at an explicit restart handoff.
#
# Usage:
#   ./scripts/install-pi.sh              # build + link + verify
#   ./scripts/install-pi.sh --skip-build # link + verify only (assumes dist/ is current)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
PI_MONO="$ROOT_DIR/vendor/pi-mono"

if [[ "${1:-}" != "--skip-build" ]]; then
  echo "▸ Building pi-mono..."
  (cd "$PI_MONO" && npm run build)
else
  echo "▸ Skipping build (--skip-build)"
fi

echo "▸ Refreshing omegon dependencies..."
(cd "$ROOT_DIR" && npm install --install-links=false)

echo "▸ Linking omegon globally..."
(cd "$ROOT_DIR" && npm link --force 2>&1 | grep -v "^npm warn")

OMEGON_PATH=$(which omegon 2>/dev/null || echo "")
if [[ -z "$OMEGON_PATH" ]]; then
  echo "✗ 'omegon' command not found on PATH after linking"
  exit 1
fi

OMEGON_VERSION=$(omegon --version 2>/dev/null || echo "FAILED")
OMEGON_REALPATH=$(python3 - <<'PY' "$OMEGON_PATH"
import os, sys
print(os.path.realpath(sys.argv[1]))
PY
)
OMEGON_WHERE=$(omegon --where 2>/dev/null || true)

PI_PATH=$(which pi 2>/dev/null || echo "")
PI_REALPATH=""
if [[ -n "$PI_PATH" ]]; then
  PI_REALPATH=$(python3 - <<'PY' "$PI_PATH"
import os, sys
print(os.path.realpath(sys.argv[1]))
PY
)
fi

echo ""
echo "✓ omegon $OMEGON_VERSION"
echo "  → $OMEGON_PATH"
echo "  ↳ $OMEGON_REALPATH"

if [[ -z "$OMEGON_WHERE" ]]; then
  echo "✗ Active omegon binary did not return Omegon runtime metadata"
  exit 1
fi

echo "$OMEGON_WHERE"

if echo "$OMEGON_REALPATH" | grep -q 'omegon' && echo "$OMEGON_WHERE" | grep -q '"omegonRoot"'; then
  echo "✓ Active omegon resolves to omegon"
else
  echo "✗ Active omegon does not appear to resolve to omegon"
  exit 1
fi

if [[ -n "$PI_PATH" ]]; then
  echo "✓ Legacy pi alias remains available"
  echo "  → $PI_PATH"
  echo "  ↳ $PI_REALPATH"
fi

echo ""
echo "✓ Lifecycle complete. Restart Omegon to pick up the rebuilt runtime."
