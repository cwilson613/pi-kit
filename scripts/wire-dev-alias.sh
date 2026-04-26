#!/usr/bin/env bash
# Wire ~/.omegon/dev-alias.sh into the user's shell profile.
# Called by `just link`. Idempotent — skips if already present.
set -euo pipefail

ALIAS_FILE="${1:?usage: wire-dev-alias.sh <alias-file>}"

SHELL_RC=""
CURRENT_SHELL="$(basename "${SHELL:-unknown}")"
if [ "$CURRENT_SHELL" = "zsh" ] && [ -f "$HOME/.zshrc" ]; then
    SHELL_RC="$HOME/.zshrc"
elif [ -f "$HOME/.bashrc" ]; then
    SHELL_RC="$HOME/.bashrc"
elif [ -f "$HOME/.bash_profile" ]; then
    SHELL_RC="$HOME/.bash_profile"
fi

if [ -n "$SHELL_RC" ] && ! grep -q 'omegon/dev-alias.sh' "$SHELL_RC" 2>/dev/null; then
    printf '\n# Omegon dev build alias (managed by just link)\n[ -f "%s" ] && source "%s"\n' \
        "$ALIAS_FILE" "$ALIAS_FILE" >> "$SHELL_RC"
    echo "✓ Added source line to $SHELL_RC"
fi
