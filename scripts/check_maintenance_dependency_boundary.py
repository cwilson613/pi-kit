#!/usr/bin/env python3
"""Assert the maintenance companion excludes normal Omegon runtime domains."""

from __future__ import annotations

import subprocess
import sys


FORBIDDEN = {
    "ansi-to-tui",
    "crossterm",
    "omegon",
    "omegon-codescan",
    "omegon-extension",
    "omegon-git",
    "omegon-memory",
    "omegon-opsx",
    "omegon-rbac",
    "omegon-secrets",
    "omegon-skills",
    "omegon-traits",
    "omegon-web",
    "ratatui",
    "rmcp",
    "styrene-work-model",
    "styrene-work-runtime",
    "tachyonfx",
}


def forbidden_packages(tree: str) -> list[str]:
    packages = {line.split()[0] for line in tree.splitlines() if line.split()}
    return sorted(packages & FORBIDDEN)


def main() -> int:
    result = subprocess.run(
        [
            "cargo",
            "tree",
            "-p",
            "omegon-maintain",
            "--locked",
            "--edges",
            "normal,build",
            "--prefix",
            "none",
        ],
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if result.returncode != 0:
        sys.stderr.write(result.stderr)
        return result.returncode

    found = forbidden_packages(result.stdout)
    if found:
        print("Maintenance dependency boundary violated:", file=sys.stderr)
        for package in found:
            print(f"  {package}", file=sys.stderr)
        return 1

    print("Maintenance dependency boundary clean: normal runtime domains are absent.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
