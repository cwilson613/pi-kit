#!/usr/bin/env python3
"""Prevent removed composition authorities from returning."""

from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SRC = ROOT / "core/crates/omegon/src"
FORBIDDEN = {
    "type DisabledTools": "disabled-name set authority",
    "set_disabled_tools": "disabled-name set publication",
    "ExtensionSupervisorSet": "duplicate extension supervisor authority",
    "McpSupervisorSet": "duplicate MCP supervisor authority",
    "won tool-name arbitration": "collision-by-order authority",
}


def main() -> int:
    findings = []
    for path in SRC.rglob("*.rs"):
        production = path.read_text().split("#[cfg(test)]", 1)[0]
        for marker, authority in FORBIDDEN.items():
            if marker in production:
                findings.append(f"{path.relative_to(ROOT)}: {authority}: {marker}")
    registry = SRC / "command_registry.rs"
    for path in SRC.rglob("*.rs"):
        if path == registry:
            continue
        production = path.read_text().split("#[cfg(test)]", 1)[0]
        if "static BUILTIN_COMMANDS" in production or "const BUILTIN_COMMANDS" in production:
            findings.append(f"{path.relative_to(ROOT)}: duplicate command registry authority")
    if findings:
        print("composition authority guard failed:\n" + "\n".join(findings))
        return 1
    print("composition authority guard passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
