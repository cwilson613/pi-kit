#!/usr/bin/env python3
"""Helpers for release automation UX and status detection."""

from __future__ import annotations

import argparse
import subprocess
import sys
from pathlib import Path


def detect_signing_status(codesign_output: str) -> str:
    if "Authority=Developer ID Application:" in codesign_output:
        return "Developer ID (YubiKey)"
    if "Authority=Omegon Local Dev" in codesign_output or "Omegon Local Dev" in codesign_output:
        return "Omegon Local Dev (self-signed)"
    if "Signature=adhoc" in codesign_output:
        return "ad-hoc"
    return "unsigned"


def read_codesign_output(binary: Path) -> str:
    try:
        completed = subprocess.run(
            ["codesign", "-dvvv", str(binary)],
            check=False,
            capture_output=True,
            text=True,
        )
    except FileNotFoundError:
        return ""
    return (completed.stdout or "") + (completed.stderr or "")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path, required=True)
    args = parser.parse_args(argv)

    output = read_codesign_output(args.binary)
    print(detect_signing_status(output))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
