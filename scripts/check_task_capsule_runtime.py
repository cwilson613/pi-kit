#!/usr/bin/env python3
"""Exercise task-capsule identity and pre-authority admission boundaries."""

from __future__ import annotations

import argparse
import json
import os
import signal
import subprocess
import tempfile
from pathlib import Path


def execute(
    command: list[str], cwd: Path, env: dict[str, str], label: str
) -> subprocess.CompletedProcess[str]:
    process = subprocess.Popen(
        command,
        cwd=cwd,
        env=env,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        start_new_session=True,
    )
    try:
        stdout, stderr = process.communicate(timeout=30)
    except subprocess.TimeoutExpired:
        os.killpg(process.pid, signal.SIGTERM)
        try:
            stdout, stderr = process.communicate(timeout=2)
        except subprocess.TimeoutExpired:
            os.killpg(process.pid, signal.SIGKILL)
            stdout, stderr = process.communicate()
        raise SystemExit(
            f"{label} timed out after 30s\nstdout:\n{stdout}\nstderr:\n{stderr}"
        )
    return subprocess.CompletedProcess(command, process.returncode, stdout, stderr)


def run(binary: Path, root: Path, profile: str) -> subprocess.CompletedProcess[str]:
    home = root / "home"
    workspace = root / "workspace"
    home.mkdir()
    workspace.mkdir()
    env = {
        **os.environ,
        "HOME": str(home),
        "OMEGON_HOME": str(home / ".omegon"),
        "OMEGON_AUTH_JSON_PATH": str(home / ".omegon" / "auth.json"),
        "RUST_LOG": "error",
    }
    return execute(
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
        label=f"task capsule profile {profile!r}",
    )


def run_invalid_task(binary: Path, root: Path) -> subprocess.CompletedProcess[str]:
    home = root / "home"
    workspace = root / "workspace"
    home.mkdir()
    workspace.mkdir()
    task_spec = root / "invalid-task.toml"
    task_spec.write_text(
        '[task]\nprompt = "must not execute"\ninvalid_policy = true\n',
        encoding="utf-8",
    )
    env = {
        **os.environ,
        "HOME": str(home),
        "OMEGON_HOME": str(home / ".omegon"),
        "OMEGON_AUTH_JSON_PATH": str(home / ".omegon" / "auth.json"),
        "RUST_LOG": "error",
    }
    return execute(
        [str(binary), "--cwd", str(workspace), "run", str(task_spec)],
        cwd=workspace,
        env=env,
        label="task capsule invalid-task admission",
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

    with tempfile.TemporaryDirectory(prefix="omegon-task-capsule-invalid-") as directory:
        root = Path(directory)
        result = run_invalid_task(binary, root)
        if result.returncode == 0 or "invalid_policy" not in result.stderr:
            raise SystemExit("task capsule did not identify an invalid task field")
        if result.stdout.strip():
            raise SystemExit("invalid task admission unexpectedly produced a run result")
        if any((root / "workspace").iterdir()) or (root / "home" / ".omegon").exists():
            raise SystemExit("invalid task produced runtime authority before refusal")

    print("Task capsule runtime identity and pre-authority refusals verified.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
