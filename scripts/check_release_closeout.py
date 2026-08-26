#!/usr/bin/env python3
"""Validate and optionally execute the deterministic Slice-7 closeout campaign."""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
EVIDENCE = ROOT / "fixtures/release-closeout-evidence-v1.json"
DIAGNOSTIC_TESTS = ROOT / "core/crates/omegon-maintain/tests/diagnostics_blackbox.rs"
RELEASE_TESTS = ROOT / "core/crates/omegon-maintain/tests/release_verify_blackbox.rs"


def load_and_validate(path: Path = EVIDENCE) -> dict:
    evidence = json.loads(path.read_text())
    if evidence.get("schema_version") != 1:
        raise ValueError("release closeout evidence must use schema version 1")
    if evidence.get("scope") != "selective-kernel-decomposition-slice-7-closeout":
        raise ValueError("release closeout evidence has the wrong scope")

    signing = evidence.get("release_signing_evidence", {})
    if signing.get("claim") != "checked_fixture_and_tests_only":
        raise ValueError("release evidence must not claim live signing")
    for field in ("fixture", "fixture_provenance"):
        if not (ROOT / signing.get(field, "")).is_file():
            raise ValueError(f"release signing evidence is missing {field}")

    rust_tests = DIAGNOSTIC_TESTS.read_text() + RELEASE_TESTS.read_text()
    required_cases = {
        "identity-composition-isolation",
        "doctor-root-refusal",
        "contribution-denial-settlement",
        "contribution-quarantine-settlement",
        "session-quarantine-settlement",
        "resource-stale-prune-and-refusal",
        "audit-success-and-corruption-refusal",
        "durable-restart-settlement",
        "managed-resource-cleanup",
        "dynamic-contribution-cleanup",
    }
    cases = evidence.get("maintenance_cases", [])
    if {case.get("id") for case in cases} != required_cases:
        raise ValueError("maintenance closeout cases are incomplete or duplicated")
    production_source = "\n".join(
        path.read_text()
        for path in (ROOT / "core/crates/omegon/src").rglob("*.rs")
    )
    for case in cases:
        test = case.get("test", "")
        if test not in rust_tests and test not in production_source:
            raise ValueError(f"closeout case references a missing test or campaign: {test}")
    for field in ("success_test", "refusal_test"):
        if signing.get(field, "") not in rust_tests:
            raise ValueError(f"release evidence references a missing {field}")

    for group, keys in evidence.get("canonical_snippets", {}).items():
        snippet = (ROOT / f"site/snippets/{group}.yaml").read_text()
        for key in keys:
            if f"\n{key}:\n" not in f"\n{snippet}":
                raise ValueError(f"canonical snippet is missing: {group}.{key}")

    for relative in evidence.get("public_pages", []):
        page = ROOT / relative
        if not page.is_file():
            raise ValueError(f"public closeout page is missing: {relative}")
    for relative in evidence.get("package_surfaces", []):
        surface = ROOT / relative
        if not surface.is_file() or "omegon-maintain" not in surface.read_text():
            raise ValueError(f"package surface omits the maintenance companion: {relative}")
    return evidence


def run_json(binary: Path, args: list[str], root: Path) -> tuple[int, dict]:
    result = subprocess.run(
        [str(binary), "--json", "--home", str(root / "home"), "--config-home", str(root / "config"), *args],
        cwd=root,
        env={key: value for key, value in os.environ.items() if not key.startswith("OMEGON_")},
        capture_output=True,
        text=True,
        timeout=30,
    )
    try:
        payload = json.loads(result.stdout)
    except json.JSONDecodeError as error:
        raise ValueError(f"maintenance command emitted invalid JSON: {result.stderr.strip()}") from error
    return result.returncode, payload


def exercise_binary(binary: Path) -> None:
    binary = binary.resolve()
    if not binary.is_file() or not os.access(binary, os.X_OK):
        raise ValueError(f"maintenance binary is not executable: {binary}")
    with tempfile.TemporaryDirectory(prefix="omegon-release-closeout-") as directory:
        root = Path(directory)
        (root / "home").mkdir()
        (root / "config").mkdir()
        for args in (["identity"], ["doctor"], ["composition", "inspect"]):
            code, payload = run_json(binary, list(args), root)
            if code not in (0, 2) or payload.get("status") not in ("success", "degraded"):
                raise ValueError(f"maintenance diagnosis failed: {' '.join(args)}")
        code, payload = run_json(binary, ["contribution", "enable", "plugin:test"], root)
        if code != 1 or payload.get("status") != "failure":
            raise ValueError("maintenance accepted a deferred contribution operation")


def run_campaign_tests() -> None:
    for test in ("diagnostics_blackbox", "release_verify_blackbox"):
        subprocess.run(
            ["cargo", "test", "-p", "omegon-maintain", "--test", test, "--locked", "--", "--test-threads=1"],
            cwd=ROOT,
            check=True,
        )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--evidence", type=Path, default=EVIDENCE)
    parser.add_argument("--maintain", type=Path)
    parser.add_argument("--run-maintenance-tests", action="store_true")
    args = parser.parse_args()
    try:
        load_and_validate(args.evidence)
        if args.maintain:
            exercise_binary(args.maintain)
        if args.run_maintenance_tests:
            run_campaign_tests()
    except (OSError, ValueError, json.JSONDecodeError, subprocess.SubprocessError) as error:
        print(f"release closeout validation failed: {error}")
        return 1
    print("release closeout evidence passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
