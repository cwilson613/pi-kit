#!/usr/bin/env python3
"""Launch and identity-check a release-coupled Omegon binary pair."""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile


VERSION_RE = re.compile(r"^omegon\s+(\S+)\s+\((\S+)\s+[^)]+\)$")
MAINTENANCE_EXCLUSIONS = [
    "default_loop",
    "extension_runtime",
    "lifecycle",
    "mcp",
    "memory",
    "mutable_packs",
    "orchestration",
    "project_config",
    "project_contributions",
    "provider_clients",
    "tui",
]


def run(command: list[str], env: dict[str, str]) -> str:
    completed = subprocess.run(command, env=env, text=True, capture_output=True)
    if completed.returncode != 0:
        detail = completed.stderr.strip() or completed.stdout.strip()
        raise RuntimeError(f"{' '.join(command)} failed: {detail}")
    return completed.stdout.strip()


def validate(omegon: Path, maintain: Path, expected_target: str | None = None) -> None:
    for binary in (omegon, maintain):
        if not binary.is_file() or not os.access(binary, os.X_OK):
            raise RuntimeError(f"required executable is missing or not executable: {binary}")

    with tempfile.TemporaryDirectory(prefix="omegon-companion-") as temp:
        root = Path(temp)
        env = os.environ.copy()
        env.update({
            "HOME": str(root),
            "XDG_CONFIG_HOME": str(root / "config"),
            "OMEGON_HOME": str(root / "omegon"),
        })
        version_output = run([str(omegon), "--version"], env).splitlines()[0]
        match = VERSION_RE.match(version_output)
        if not match:
            raise RuntimeError(f"unexpected omegon version output: {version_output!r}")
        omegon_version, omegon_commit = match.groups()

        identity = json.loads(run([str(maintain), "--json", "identity"], env))
        if identity.get("status") != "success":
            raise RuntimeError("omegon-maintain identity did not report success")
        artifact = identity.get("artifact", {})
        composition = identity.get("composition", {})
        if composition.get("profile") != "maintenance":
            raise RuntimeError("omegon-maintain did not report the maintenance profile")
        if composition.get("excluded_inputs") != MAINTENANCE_EXCLUSIONS:
            raise RuntimeError("omegon-maintain reported an unexpected exclusion set")
        if artifact.get("version") != omegon_version:
            raise RuntimeError(
                f"companion version mismatch: omegon={omegon_version}, "
                f"omegon-maintain={artifact.get('version')}"
            )
        maintain_commit = artifact.get("commit")
        comparable_omegon_commit = omegon_commit.removesuffix("-dirty")
        comparable_maintain_commit = (
            maintain_commit.removesuffix("-dirty") if maintain_commit else maintain_commit
        )
        if (
            "unknown" not in (comparable_omegon_commit, comparable_maintain_commit)
            and comparable_maintain_commit != comparable_omegon_commit
        ):
            raise RuntimeError(
                f"companion commit mismatch: omegon={omegon_commit}, "
                f"omegon-maintain={maintain_commit}"
            )
        if expected_target and artifact.get("target") != expected_target:
            raise RuntimeError(
                f"companion target mismatch: expected={expected_target}, "
                f"actual={artifact.get('target')}"
            )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--omegon", type=Path, required=True)
    parser.add_argument("--maintain", type=Path, required=True)
    parser.add_argument("--target")
    args = parser.parse_args()
    try:
        validate(args.omegon, args.maintain, args.target)
    except (OSError, RuntimeError, ValueError, json.JSONDecodeError) as error:
        print(f"companion validation failed: {error}", file=sys.stderr)
        return 1
    print(f"validated release-coupled companions: {args.omegon} + {args.maintain}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
