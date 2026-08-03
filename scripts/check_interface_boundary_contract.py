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
SURFACES = SRC / "surfaces"
TUI = SRC / "tui"
FRONTEND_ENTRYPOINTS = [
    SRC / "acp.rs",
    SRC / "acp_worker.rs",
    SRC / "ipc" / "connection.rs",
    SRC / "web" / "mod.rs",
    SRC / "web" / "ws.rs",
]

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


FORBIDDEN_SURFACE_PATTERNS = FORBIDDEN_UI_RUNTIME_PATTERNS

FORBIDDEN_TUI_PATTERNS = [
    re.compile(r"\bcrate::control_runtime\b"),
    re.compile(r"\bcontrol_runtime::"),
]

FORBIDDEN_FRONTEND_PATTERNS = [
    re.compile(r"\bcrate::control_runtime::ControlRequest\b"),
    re.compile(r"\bcontrol_runtime::ControlRequest\b"),
]


def rust_files(root: Path) -> list[Path]:
    return sorted(path for path in root.rglob("*.rs") if path.is_file())


def check_patterns(root: Path, patterns: list[re.Pattern[str]], label: str) -> list[str]:
    violations: list[str] = []
    for path in rust_files(root):
        rel = path.relative_to(ROOT)
        for line_no, line in enumerate(path.read_text().splitlines(), start=1):
            for pattern in patterns:
                if pattern.search(line):
                    violations.append(
                        f"{rel}:{line_no}: forbidden {label} dependency `{pattern.pattern}`: {line.strip()}"
                    )
    return violations


def check_ui_runtime_is_neutral() -> list[str]:
    return check_patterns(UI_RUNTIME, FORBIDDEN_UI_RUNTIME_PATTERNS, "ui_runtime")


def check_surfaces_are_neutral() -> list[str]:
    return check_patterns(SURFACES, FORBIDDEN_SURFACE_PATTERNS, "surface")


def check_tui_uses_interface_boundary() -> list[str]:
    return check_patterns(TUI, FORBIDDEN_TUI_PATTERNS, "tui")


def check_frontends_name_interface_control_request() -> list[str]:
    violations: list[str] = []
    for path in FRONTEND_ENTRYPOINTS:
        rel = path.relative_to(ROOT)
        for line_no, line in enumerate(path.read_text().splitlines(), start=1):
            for pattern in FORBIDDEN_FRONTEND_PATTERNS:
                if pattern.search(line):
                    violations.append(
                        f"{rel}:{line_no}: forbidden frontend request dependency `{pattern.pattern}`: {line.strip()}"
                    )
    return violations


def main() -> int:
    violations = []
    violations.extend(check_ui_runtime_is_neutral())
    violations.extend(check_surfaces_are_neutral())
    violations.extend(check_tui_uses_interface_boundary())
    violations.extend(check_frontends_name_interface_control_request())
    if violations:
        print("InterfaceBoundary contract violated:")
        for violation in violations:
            print(f"  {violation}")
        return 1

    print("InterfaceBoundary contract clean: semantic surfaces are neutral and frontends name the boundary.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
