#!/usr/bin/env python3
"""Assert the UI InterfaceBoundary dependency direction stays renderer-neutral.

The boundary contract is:
  frontend adapters (for example Ratatui TUI) -> ui_runtime/surfaces/operator_commands -> runtime

This check intentionally starts with the hard invariants that should never
regress while the remaining TUI backend references are extracted in slices.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SRC = ROOT / "core" / "crates" / "omegon" / "src"
UI_RUNTIME = SRC / "ui_runtime"

FORBIDDEN_UI_RUNTIME_PATTERNS = [
    re.compile(r"\bcrate::tui\b"),
    re.compile(r"\bcrate::control_runtime\b"),
    re.compile(r"\bcrate::interactive_coordinator\b"),
    re.compile(r"\bcrate::runtime_state\b"),
    re.compile(r"\bratatui\b"),
    re.compile(r"\bcrossterm\b"),
    re.compile(r"\btachyonfx\b"),
    re.compile(r"\bratatui_textarea\b"),
    re.compile(r"\btui_tree_widget\b"),
]


def rust_files(root: Path) -> list[Path]:
    return sorted(path for path in root.rglob("*.rs") if path.is_file())


def check_ui_runtime_is_neutral() -> list[str]:
    violations: list[str] = []
    for path in rust_files(UI_RUNTIME):
        rel = path.relative_to(ROOT)
        for line_no, line in enumerate(path.read_text().splitlines(), start=1):
            for pattern in FORBIDDEN_UI_RUNTIME_PATTERNS:
                if pattern.search(line):
                    violations.append(f"{rel}:{line_no}: forbidden boundary dependency `{pattern.pattern}`: {line.strip()}")
    return violations


def main() -> int:
    violations = check_ui_runtime_is_neutral()
    if violations:
        print("InterfaceBoundary contract violated:")
        for violation in violations:
            print(f"  {violation}")
        return 1

    print("InterfaceBoundary contract clean: ui_runtime is renderer-neutral and backend-internal-free.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
