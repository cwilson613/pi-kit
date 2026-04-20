#!/usr/bin/env bash
# bootstrap.sh — Set up the Omegon development environment from scratch.
#
# Usage:
#   ./scripts/bootstrap.sh          # Full setup: toolchain + build + link
#   ./scripts/bootstrap.sh --check  # Just verify prerequisites, don't install
#
# What it does:
#   1. Installs Homebrew (macOS) if missing
#   2. Installs just (command runner) if missing
#   3. Installs Rust via rustup if missing
#   4. Builds omegon (dev-release profile for fast iteration)
#   5. Links the binary onto PATH (omegon + om aliases)
#   6. Runs a quick sanity check
#
# Safe to re-run — each step is idempotent.

set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
BOLD='\033[1m'
RESET='\033[0m'

info()  { echo -e "${GREEN}[ok]${RESET}  $*"; }
warn()  { echo -e "${YELLOW}[!!]${RESET}  $*"; }
fail()  { echo -e "${RED}[err]${RESET} $*"; exit 1; }
step()  { echo -e "\n${BOLD}── $* ──${RESET}"; }

CHECK_ONLY=false
if [[ "${1:-}" == "--check" ]]; then
    CHECK_ONLY=true
fi

# ─── Locate project root ───────────────────────────────────────
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$PROJECT_ROOT"

# ─── 1. Package manager ────────────────────────────────────────
step "Package manager"

if [[ "$OSTYPE" == darwin* ]]; then
    if command -v brew &>/dev/null; then
        info "Homebrew $(brew --version | head -1 | awk '{print $2}')"
    elif $CHECK_ONLY; then
        warn "Homebrew not installed"
    else
        echo "Installing Homebrew..."
        /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
        # Add to current session
        eval "$(/opt/homebrew/bin/brew shellenv 2>/dev/null || /usr/local/bin/brew shellenv 2>/dev/null)"
        info "Homebrew installed"
    fi
elif [[ "$OSTYPE" == linux* ]]; then
    if command -v apt-get &>/dev/null; then
        info "apt available"
    elif command -v dnf &>/dev/null; then
        info "dnf available"
    else
        warn "No recognized package manager — you may need to install dependencies manually"
    fi
fi

# ─── 2. just (command runner) ───────────────────────────────────
step "just"

if command -v just &>/dev/null; then
    info "just $(just --version 2>/dev/null | awk '{print $2}')"
elif $CHECK_ONLY; then
    warn "just not installed"
else
    echo "Installing just..."
    if command -v brew &>/dev/null; then
        brew install just
    elif command -v cargo &>/dev/null; then
        cargo install just
    else
        curl --proto '=https' --tlsv1.2 -sSf https://just.systems/install.sh | bash -s -- --to ~/.local/bin
        export PATH="$HOME/.local/bin:$PATH"
    fi
    info "just installed"
fi

# ─── 3. Rust toolchain ─────────────────────────────────────────
step "Rust toolchain"

if command -v rustup &>/dev/null; then
    RUST_VER="$(rustc --version 2>/dev/null | awk '{print $2}')"
    info "Rust $RUST_VER (via rustup)"

    # Ensure stable is installed and up to date
    if ! $CHECK_ONLY; then
        rustup default stable &>/dev/null
        echo "  Checking for updates..."
        rustup update stable 2>/dev/null | grep -v "unchanged" || true
    fi
elif command -v rustc &>/dev/null; then
    RUST_VER="$(rustc --version | awk '{print $2}')"
    warn "Rust $RUST_VER found but not via rustup — some features may not work"
    warn "Install rustup: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
elif $CHECK_ONLY; then
    warn "Rust not installed"
else
    echo "Installing Rust via rustup..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
    # shellcheck disable=SC1091
    source "$HOME/.cargo/env"
    info "Rust $(rustc --version | awk '{print $2}') installed"
fi

# Verify cargo is available
if ! command -v cargo &>/dev/null; then
    # Try sourcing cargo env in case it was just installed
    # shellcheck disable=SC1091
    [[ -f "$HOME/.cargo/env" ]] && source "$HOME/.cargo/env"
    if ! command -v cargo &>/dev/null; then
        fail "cargo not found. Restart your shell or run: source \$HOME/.cargo/env"
    fi
fi

# ─── 4. Optional: clippy + rustfmt ─────────────────────────────
step "Rust components"

if rustup component list --installed 2>/dev/null | grep -q clippy; then
    info "clippy installed"
else
    if ! $CHECK_ONLY; then
        rustup component add clippy
        info "clippy installed"
    else
        warn "clippy not installed"
    fi
fi

if rustup component list --installed 2>/dev/null | grep -q rustfmt; then
    info "rustfmt installed"
else
    if ! $CHECK_ONLY; then
        rustup component add rustfmt
        info "rustfmt installed"
    else
        warn "rustfmt not installed"
    fi
fi

# ─── 5. PKL (configuration language) ────────────────────────────
step "PKL"

if command -v pkl &>/dev/null; then
    info "pkl $(pkl --version 2>/dev/null | head -1)"
elif $CHECK_ONLY; then
    warn "pkl not installed (needed for custom posture/agent configs)"
else
    echo "Installing pkl..."
    if command -v brew &>/dev/null; then
        brew install pkl
    else
        # Direct download for non-Homebrew systems
        warn "Install pkl manually: https://pkl-lang.org/main/current/pkl-cli/index.html"
    fi
    if command -v pkl &>/dev/null; then
        info "pkl installed"
    fi
fi

# ─── 6. Build ───────────────────────────────────────────────────
if $CHECK_ONLY; then
    step "Summary"
    echo "Run without --check to install missing components and build."
    exit 0
fi

step "Building omegon (dev-release)"
echo "  This uses thin LTO for fast linking (~90% of release performance)."
echo "  For a full release build, use: just build"
echo ""

cd "$PROJECT_ROOT/core"
cargo build --profile dev-release -p omegon

info "Build complete"

# ─── 7. Link ───────────────────────────────────────────────────
step "Linking binary"
cd "$PROJECT_ROOT"
just link

# ─── 8. Sanity check ───────────────────────────────────────────
step "Sanity check"

# Check PATH and the known install locations
OMEGON_BIN=""
if command -v omegon &>/dev/null; then
    OMEGON_BIN="$(command -v omegon)"
elif [[ -x "$HOME/.local/bin/omegon" ]]; then
    OMEGON_BIN="$HOME/.local/bin/omegon"
elif [[ -x "/usr/local/bin/omegon" ]]; then
    OMEGON_BIN="/usr/local/bin/omegon"
fi

if [[ -n "$OMEGON_BIN" ]]; then
    INSTALLED_VER="$("$OMEGON_BIN" --version 2>/dev/null)"
    info "$INSTALLED_VER"
    # Remind about PATH if it's not on the default PATH
    if ! command -v omegon &>/dev/null; then
        warn "omegon is installed at $OMEGON_BIN but not on your current PATH"
        echo "  Add this to your shell profile (~/.zshrc or ~/.bashrc):"
        echo "    export PATH=\"\$HOME/.local/bin:\$PATH\""
        echo "  Then restart your shell or run: source ~/.zshrc"
    fi
else
    warn "omegon binary not found — build may have failed"
fi

echo ""
echo -e "${GREEN}${BOLD}Ready to develop!${RESET}"
echo ""
echo "  Quick reference:"
echo "    just check        Type-check (fast)"
echo "    just lint         Clippy + check"
echo "    just test-rust    Run all tests"
echo "    just build        Full release build"
echo "    just update       Pull + dev-release build"
echo ""
