#!/usr/bin/env bash
# Build omegon-agent for the current platform and package it into the
# corresponding npm platform package directory.
#
# Usage:
#   ./scripts/build-platform-packages.sh          # build for current platform
#   ./scripts/build-platform-packages.sh --target aarch64-apple-darwin  # cross-compile
#
# The binary is placed into npm/platform-packages/<platform>/omegon-agent
# and the package.json version is synced from the root package.json.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
CORE_DIR="$REPO_ROOT/core"

# Parse args
TARGET=""
if [[ "${1:-}" == "--target" && -n "${2:-}" ]]; then
  TARGET="$2"
fi

# Detect current platform if no target specified
if [[ -z "$TARGET" ]]; then
  case "$(uname -s)-$(uname -m)" in
    Darwin-arm64)  TARGET="aarch64-apple-darwin" ;;
    Darwin-x86_64) TARGET="x86_64-apple-darwin" ;;
    Linux-x86_64)  TARGET="x86_64-unknown-linux-gnu" ;;
    Linux-aarch64) TARGET="aarch64-unknown-linux-gnu" ;;
    *) echo "Unsupported platform: $(uname -s)-$(uname -m)"; exit 1 ;;
  esac
fi

# Map target to platform package name
case "$TARGET" in
  aarch64-apple-darwin)       PLATFORM="darwin-arm64" ;;
  x86_64-apple-darwin)        PLATFORM="darwin-x64" ;;
  x86_64-unknown-linux-gnu)   PLATFORM="linux-x64" ;;
  aarch64-unknown-linux-gnu)  PLATFORM="linux-arm64" ;;
  *) echo "Unknown target: $TARGET"; exit 1 ;;
esac

PKG_DIR="$REPO_ROOT/npm/platform-packages/$PLATFORM"
VERSION=$(node -p "require('$REPO_ROOT/package.json').version")

echo "Building omegon-agent"
echo "  Target:   $TARGET"
echo "  Platform: $PLATFORM"
echo "  Version:  $VERSION"
echo "  Output:   $PKG_DIR/omegon-agent"
echo ""

# Ensure the target is installed
rustup target add "$TARGET" 2>/dev/null || true

# Build
cd "$CORE_DIR"
cargo build --release --target "$TARGET"

# Copy binary
BINARY="$CORE_DIR/target/$TARGET/release/omegon-agent"
if [[ ! -f "$BINARY" ]]; then
  echo "ERROR: Binary not found at $BINARY"
  exit 1
fi
cp "$BINARY" "$PKG_DIR/omegon-agent"
chmod +x "$PKG_DIR/omegon-agent"

# Update version in platform package.json
node -e "
const p = require('$PKG_DIR/package.json');
p.version = '$VERSION';
require('fs').writeFileSync('$PKG_DIR/package.json', JSON.stringify(p, null, 2) + '\n');
"

SIZE=$(du -h "$PKG_DIR/omegon-agent" | cut -f1)
echo ""
echo "✓ Built $PLATFORM binary: $SIZE"
echo "  Package: @styrene-lab/omegon-$PLATFORM@$VERSION"
