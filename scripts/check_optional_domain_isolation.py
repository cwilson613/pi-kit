#!/usr/bin/env python3
"""Validate the Slice-6 optional-domain isolation and documentation matrix."""

from __future__ import annotations

import re
import tomllib
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
MATRIX = (
    ROOT
    / "openspec/archive/2026-08-26-selective-kernel-decomposition/fixtures/optional-domain-proof-v1.toml"
)
EXPECTED_DOMAINS = {
    "plans-work",
    "behavior-policy",
    "codescan",
    "lifecycle-openspec",
    "memory",
    "context-compaction",
    "git",
    "dynamic-contributions",
    "shipped-content",
}


def require_marker(entry: dict[str, str], label: str, errors: list[str]) -> None:
    path = ROOT / entry["path"]
    if not path.is_file():
        errors.append(f"{label}: missing {entry['path']}")
        return
    if entry["marker"] not in path.read_text():
        errors.append(f"{label}: marker {entry['marker']!r} missing from {entry['path']}")


def require_test(entry: dict[str, str], label: str, errors: list[str]) -> None:
    path = ROOT / entry["path"]
    if not path.is_file():
        errors.append(f"{label}: missing {entry['path']}")
        return
    pattern = rf"\b(?:async\s+)?fn\s+{re.escape(entry['test'])}\s*\("
    if re.search(pattern, path.read_text()) is None:
        errors.append(f"{label}: test {entry['test']!r} missing from {entry['path']}")


def main() -> int:
    matrix = tomllib.loads(MATRIX.read_text())
    errors: list[str] = []
    domains = matrix.get("domains", [])
    ids = [domain.get("id") for domain in domains]
    if set(ids) != EXPECTED_DOMAINS or len(ids) != len(EXPECTED_DOMAINS):
        errors.append(f"domain inventory mismatch: {ids!r}")

    maintenance_guard = (ROOT / "scripts/check_maintenance_dependency_boundary.py").read_text()
    kernel_sources = [(ROOT / path, path) for path in matrix.get("kernel_sources", [])]
    for path, relative in kernel_sources:
        if not path.is_file():
            errors.append(f"kernel source missing: {relative}")

    for domain in domains:
        domain_id = domain["id"]
        label = f"domain {domain_id}"
        if not domain.get("composition"):
            errors.append(f"{label}: composition classification is empty")
        require_marker(domain["architecture"], f"{label} architecture", errors)
        require_test(domain["absence"], f"{label} absence", errors)
        require_test(domain["degradation"], f"{label} degradation", errors)

        public = domain["public"]
        if public["status"] == "documented":
            require_marker(public, f"{label} public docs", errors)
        elif public["status"] == "not-applicable":
            if len(public.get("reason", "").strip()) < 40:
                errors.append(f"{label}: no-public-change rationale is missing or too vague")
        else:
            errors.append(f"{label}: invalid public documentation status {public['status']!r}")

        for package in domain.get("maintenance_packages", []):
            if f'"{package}"' not in maintenance_guard:
                errors.append(f"{label}: {package} is not enforced by the maintenance guard")

        for path, relative in kernel_sources:
            if not path.is_file():
                continue
            source = path.read_text()
            for token in domain.get("kernel_tokens", []):
                if token in source:
                    errors.append(f"{label}: optional token {token!r} leaked into {relative}")

    if errors:
        raise SystemExit("\n".join(errors))
    print(f"Optional-domain proof matrix clean: {len(domains)} domains checked.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
