#!/usr/bin/env python3
"""Check upstream CLI versions and compare against hardcoded values in Omegon.

Checks:
  - Claude Code CLI version (npm: @anthropic-ai/claude-code)

Outputs JSON: {"updates": [{"name": ..., "current": ..., "latest": ..., "file": ..., "pattern": ...}]}
Exit code 0 = all up to date, 1 = updates available, 2 = check failed.
"""

import json
import re
import subprocess
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
PROVIDERS_RS = REPO_ROOT / "core" / "crates" / "omegon" / "src" / "providers.rs"


def get_npm_latest(package: str) -> str | None:
    """Get latest version of an npm package."""
    try:
        result = subprocess.run(
            ["npm", "view", package, "version"],
            capture_output=True, text=True, timeout=30,
        )
        if result.returncode == 0:
            return result.stdout.strip()
    except (FileNotFoundError, subprocess.TimeoutExpired):
        pass
    return None


def get_current_claude_code_ua() -> str | None:
    """Extract the Claude Code UA version from providers.rs."""
    content = PROVIDERS_RS.read_text()
    m = re.search(r'CLAUDE_CODE_UA:\s*&str\s*=\s*"claude-cli/([^"]+)"', content)
    return m.group(1) if m else None


def check_claude_code() -> dict | None:
    """Check if Claude Code CLI version is up to date."""
    current = get_current_claude_code_ua()
    if not current:
        print("WARNING: Could not extract current Claude Code UA version", file=sys.stderr)
        return None

    latest = get_npm_latest("@anthropic-ai/claude-code")
    if not latest:
        print("WARNING: Could not fetch latest Claude Code version from npm", file=sys.stderr)
        return None

    if current != latest:
        return {
            "name": "Claude Code CLI",
            "current": current,
            "latest": latest,
            "file": str(PROVIDERS_RS.relative_to(REPO_ROOT)),
            "pattern": f'claude-cli/{current}',
            "replacement": f'claude-cli/{latest}',
        }
    return None


def main():
    updates = []

    result = check_claude_code()
    if result:
        updates.append(result)

    output = {"updates": updates}
    print(json.dumps(output, indent=2))

    if updates:
        print(f"\n{len(updates)} update(s) available:", file=sys.stderr)
        for u in updates:
            print(f"  {u['name']}: {u['current']} -> {u['latest']}", file=sys.stderr)
        sys.exit(1)
    else:
        print("\nAll upstream versions up to date.", file=sys.stderr)
        sys.exit(0)


if __name__ == "__main__":
    main()
