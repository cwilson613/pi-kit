#!/usr/bin/env sh
# Pre-install hook for omegon.
#
# Omegon is an opinionated distribution of pi (https://github.com/badlogic/pi)
# that bundles extensions, themes, skills, and memory on top of the core
# pi coding agent by Mario Zechner (@badlogic).
#
# Both omegon and the standalone pi packages (@cwilson613/pi-coding-agent,
# @mariozechner/pi-coding-agent) register a `pi` binary. npm cannot create
# a bin link if another package already owns it, so this script removes the
# standalone pi package before omegon installs — preventing an EEXIST error.
#
# This is NOT hostile. Omegon depends on and includes the same pi core.
# If you want standalone pi back, just:
#   npm uninstall -g omegon
#   npm install -g @mariozechner/pi-coding-agent
#
# Only acts during global installs (npm_config_global=true).

if [ "$npm_config_global" != "true" ]; then
  exit 0
fi

for pkg in @cwilson613/pi-coding-agent @mariozechner/pi-coding-agent; do
  if npm ls -g "$pkg" --depth=0 >/dev/null 2>&1; then
    echo ""
    echo "  omegon: Found standalone pi package ($pkg)."
    echo "  omegon: Omegon bundles pi core and registers the same 'pi' command."
    echo "  omegon: Removing $pkg to avoid bin conflict..."
    echo "  omegon: (To restore standalone pi later: npm install -g $pkg)"
    echo ""
    npm uninstall -g "$pkg" 2>/dev/null || true
  fi
done
