#!/usr/bin/env python3
"""Assert the no-default-features omegon graph excludes terminal/TUI crates."""

from __future__ import annotations

import subprocess
import sys

FORBIDDEN = [
    "ansi-to-tui",
    "crossterm",
    "hyperrat",
    "ratatui",
    "ratatui-core",
    "ratatui-image",
    "ratatui-toaster",
    "ratatui-textarea",
    "ratatui-widgets",
    "tachyonfx",
    "tui-popup",
    "tui-syntax-highlight",
    "tui-tree-widget",
]


def main() -> int:
    result = subprocess.run(
        [
            "cargo",
            "tree",
            "-p",
            "omegon",
            "--locked",
            "--no-default-features",
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

    found: dict[str, list[str]] = {}
    for line in result.stdout.splitlines():
        package = line.split()[0] if line.split() else ""
        for forbidden in FORBIDDEN:
            if package == forbidden:
                found.setdefault(forbidden, []).append(line)

    if found:
        print("Headless dependency boundary violated; forbidden TUI crates are present:")
        for forbidden, lines in found.items():
            print(f"  {forbidden}")
            for line in lines[:3]:
                print(f"    {line}")
        return 1

    print("Headless dependency boundary clean: no forbidden TUI crates found.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
