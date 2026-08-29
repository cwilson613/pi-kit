#!/usr/bin/env python3
"""Assert the task-capsule-v0 graph contains only its declared dependency layer."""

from __future__ import annotations

import subprocess
import sys
import tomllib
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


FORBIDDEN = [
    "ansi-to-tui",
    "crossterm",
    "hyperrat",
    "image",
    "omegon-codescan",
    "ratatui",
    "ratatui-core",
    "ratatui-image",
    "ratatui-toaster",
    "ratatui-textarea",
    "ratatui-widgets",
    "sigstore",
    "syntect",
    "tachyonfx",
    "tui-popup",
    "tui-syntax-highlight",
    "tui-tree-widget",
    "x509-parser",
]

DIRECT_TUI_DEPENDENCIES = {"image", "syntect", "unicode-width"}


def forbidden_packages(output: str) -> dict[str, list[str]]:
    found: dict[str, list[str]] = {}
    for line in output.splitlines():
        fields = line.split()
        package = fields[0] if fields else ""
        if package in FORBIDDEN:
            found.setdefault(package, []).append(line)
    return found


def direct_tui_ownership_errors(manifest: dict) -> list[str]:
    dependencies = manifest.get("dependencies", {})
    tui_features = set(manifest.get("features", {}).get("tui", []))
    errors = []
    for package in sorted(DIRECT_TUI_DEPENDENCIES):
        dependency = dependencies.get(package, {})
        if not isinstance(dependency, dict) or dependency.get("optional") is not True:
            errors.append(f"{package} must remain an optional direct dependency")
        if f"dep:{package}" not in tui_features:
            errors.append(f"{package} must remain owned by the tui feature")
    return errors


def main() -> int:
    manifest_path = ROOT / "core/crates/omegon/Cargo.toml"
    ownership_errors = direct_tui_ownership_errors(tomllib.loads(manifest_path.read_text()))
    if ownership_errors:
        print("Task capsule direct presentation ownership violated:", file=sys.stderr)
        for error in ownership_errors:
            print(f"  {error}", file=sys.stderr)
        return 1

    result = subprocess.run(
        [
            "cargo",
            "tree",
            "-p",
            "omegon",
            "--locked",
            "--no-default-features",
            "--features",
            "task-capsule",
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
        print("Task capsule dependency boundary violated; forbidden packages are present:")
        for package, lines in found.items():
            print(f"  {package}")
            for line in lines[:3]:
                print(f"    {line}")
        return 1

    print(
        "Task capsule dependency boundary clean: presentation, codescan engine, "
        "and self-update verification are absent; direct presentation dependencies "
        "remain TUI-owned."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
