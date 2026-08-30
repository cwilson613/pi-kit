#!/usr/bin/env python3
"""Assert the shrinking no-default Omegon graph excludes optional engines."""

from __future__ import annotations

import subprocess
import sys

SHRINKING_FORBIDDEN = [
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
    "omegon-codescan",
]

HOST_FORBIDDEN = ["omegon-codescan"]


def cargo_tree(*extra: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["cargo", "tree", "-p", "omegon", "--locked", *extra, "--prefix", "none"],
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )


def forbidden_packages(output: str, forbidden: list[str]) -> dict[str, list[str]]:
    found: dict[str, list[str]] = {}
    for line in output.splitlines():
        package = line.split()[0] if line.split() else ""
        for name in forbidden:
            if package == name:
                found.setdefault(name, []).append(line)
    return found


def main() -> int:
    checks = [
        (
            "shrinking",
            cargo_tree("--no-default-features", "--features", "product"),
            SHRINKING_FORBIDDEN,
        ),
        ("all-features host", cargo_tree("--all-features"), HOST_FORBIDDEN),
    ]
    for label, result, forbidden in checks:
        if result.returncode != 0:
            sys.stderr.write(result.stderr)
            return result.returncode
        found = forbidden_packages(result.stdout, forbidden)
        if found:
            print(f"{label.title()} dependency boundary violated; forbidden packages are present:")
            for name, lines in found.items():
                print(f"  {name}")
                for line in lines[:3]:
                    print(f"    {line}")
            return 1

    print("Omegon dependency boundaries clean: codescan engine absent from the host graph.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
