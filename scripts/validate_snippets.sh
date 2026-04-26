#!/usr/bin/env bash
# Validates canonical site snippets against the installed omegon binary.
# Run this as part of the release preflight to catch drift before publish.
#
# Usage: ./scripts/validate_snippets.sh [path-to-omegon]
set -euo pipefail

OMEGON="${1:-omegon}"
FAIL=0
PASS=0
SKIP=0

info()  { printf '  \033[32m✓\033[0m %s\n' "$1"; }
fail()  { printf '  \033[31m✗\033[0m %s\n' "$1"; FAIL=$((FAIL+1)); }
skip()  { printf '  \033[33m-\033[0m %s (skipped)\n' "$1"; SKIP=$((SKIP+1)); }

echo "Validating snippets against: $($OMEGON --version 2>&1)"
echo ""

# ── CLI subcommands ──────────────────────────────────────────────────────
echo "CLI subcommands:"
HELP=$($OMEGON --help 2>&1)

check_subcommand() {
  local sub="$1" label="$2"
  if echo "$HELP" | grep -qw "$sub"; then
    info "$label"; PASS=$((PASS+1))
  else
    fail "$label — '$sub' not found in omegon --help"
  fi
}

check_subcommand "auth"      "omegon auth"
check_subcommand "migrate"   "omegon migrate"
check_subcommand "extension" "omegon extension"
check_subcommand "serve"     "omegon serve"
check_subcommand "switch"    "omegon switch"
check_subcommand "skills"    "omegon skills"
check_subcommand "run"       "omegon run"
check_subcommand "cleave"    "omegon cleave"
check_subcommand "eval"      "omegon eval"
check_subcommand "plugin"    "omegon plugin"
check_subcommand "acp"       "omegon acp"

# ── auth subcommands ─────────────────────────────────────────────────────
echo ""
echo "Auth subcommands:"
AUTH_HELP=$($OMEGON auth --help 2>&1)

for sub in status login logout unlock; do
  if echo "$AUTH_HELP" | grep -qw "$sub"; then
    info "omegon auth $sub"; PASS=$((PASS+1))
  else
    fail "omegon auth $sub — not in auth --help"
  fi
done

# ── extension subcommands ────────────────────────────────────────────────
echo ""
echo "Extension subcommands:"
EXT_HELP=$($OMEGON extension --help 2>&1)

for sub in init install list remove update enable disable; do
  if echo "$EXT_HELP" | grep -qw "$sub"; then
    info "omegon extension $sub"; PASS=$((PASS+1))
  else
    fail "omegon extension $sub — not in extension --help"
  fi
done

# ── CLI flags ────────────────────────────────────────────────────────────
echo ""
echo "CLI flags:"

check_flag() {
  local flag="$1" label="$2"
  if echo "$HELP" | grep -q -- "$flag"; then
    info "$label"; PASS=$((PASS+1))
  else
    fail "$label — '$flag' not found in omegon --help"
  fi
}

check_flag "--resume"       "omegon --resume"
check_flag "--fresh"        "omegon --fresh"
check_flag "--no-session"   "omegon --no-session"
check_flag "--prompt "      "omegon --prompt"
check_flag "--prompt-file"  "omegon --prompt-file"
check_flag "--max-turns"    "omegon --max-turns"
check_flag "--smoke"        "omegon --smoke"
check_flag "--slim"         "omegon --slim"
check_flag "--full"         "omegon --full"

# ── Drift: verify 'omegon login' is NOT a valid subcommand ──────────────
echo ""
echo "Drift guards (things that should NOT work):"

if $OMEGON login --help >/dev/null 2>&1; then
  fail "'omegon login' should not be a top-level subcommand (use 'omegon auth login')"
else
  info "'omegon login' correctly rejected"; PASS=$((PASS+1))
fi

if $OMEGON extension new --help >/dev/null 2>&1; then
  fail "'omegon extension new' should not exist (use 'omegon extension init')"
else
  info "'omegon extension new' correctly rejected"; PASS=$((PASS+1))
fi

# ── migrate sources ──────────────────────────────────────────────────────
echo ""
echo "Migrate sources:"
MIG_HELP=$($OMEGON migrate --help 2>&1)

for src in auto claude-code codex cursor aider; do
  if echo "$MIG_HELP" | grep -qi "$src"; then
    info "omegon migrate $src"; PASS=$((PASS+1))
  else
    fail "omegon migrate $src — not found in migrate --help"
  fi
done

# ── Summary ──────────────────────────────────────────────────────────────
echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
printf "  Pass: %d  Fail: %d  Skip: %d\n" "$PASS" "$FAIL" "$SKIP"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

exit "$FAIL"
