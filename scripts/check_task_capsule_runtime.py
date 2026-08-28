#!/usr/bin/env python3
"""Exercise the built task capsule's identity and fail-closed profile boundary."""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import tempfile
from pathlib import Path


def run(binary: Path, root: Path, profile: str) -> subprocess.CompletedProcess[str]:
    home = root / "home"
    workspace = root / "workspace"
    home.mkdir()
    workspace.mkdir()
    env = {
        **os.environ,
        "HOME": str(home),
        "OMEGON_HOME": str(home / ".omegon"),
        "RUST_LOG": "error",
    }
    return subprocess.run(
        [
            str(binary),
            "--cwd",
            str(workspace),
            "composition-inspect",
            "--profile",
            profile,
        ],
        cwd=workspace,
        env=env,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=30,
        check=False,
    )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("binary", type=Path)
    args = parser.parse_args()
    binary = args.binary.resolve()
    if not binary.is_file():
        raise SystemExit(f"task capsule binary not found: {binary}")

    with tempfile.TemporaryDirectory(prefix="omegon-task-capsule-") as directory:
        result = run(binary, Path(directory), "task-capsule")
        if result.returncode != 0:
            raise SystemExit(result.stderr.strip() or "task capsule inspection failed")
        payload = json.loads(result.stdout)
        expected = {
            "artifact_profile": "task-capsule-v0",
            "canonical_entrypoint": ["omegon", "run"],
            "profile": "task-capsule",
            "runtime_mode": "bounded-task",
            "surfaces": ["agent-loop", "bounded-task"],
            "absent_optional": ["tui", "self-update"],
        }
        for field, value in expected.items():
            if payload.get(field) != value:
                raise SystemExit(f"task capsule {field} mismatch: {payload.get(field)!r}")

    with tempfile.TemporaryDirectory(prefix="omegon-task-capsule-refusal-") as directory:
        root = Path(directory)
        result = run(binary, root, "full")
        if result.returncode == 0 or "incompatible with task-capsule-v0" not in result.stderr:
            raise SystemExit("task capsule accepted an incompatible full profile")
        if any((root / "workspace").iterdir()) or (root / "home" / ".omegon").exists():
            raise SystemExit("incompatible profile produced runtime state before refusal")

    print("Task capsule runtime identity and pre-start profile refusal verified.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
