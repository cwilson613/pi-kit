#!/usr/bin/env python3
"""Measure release composition from runtime and build artifacts and enforce budgets."""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import tarfile
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
POLICY = ROOT / "fixtures/composition-budgets-v1.json"
METRICS = (
    "dependency_count",
    "binary_size_bytes",
    "startup_task_count",
    "model_schema_tokens",
    "resident_capabilities",
    "default_callable_capabilities",
)


def dependency_count(package: str, target: str) -> int:
    result = subprocess.run(
        ["cargo", "tree", "-p", package, "--target", target, "--prefix", "none", "--edges", "normal"],
        cwd=ROOT,
        capture_output=True,
        text=True,
        check=True,
    )
    packages = {line.split()[0] for line in result.stdout.splitlines() if line and not line.startswith("[")}
    return max(0, len(packages) - 1)


def extract_runtime(archive: Path, destination: Path) -> Path:
    with tarfile.open(archive, "r:gz") as package:
        for member in package:
            if not member.isfile():
                raise ValueError(f"release archive contains a non-file member: {member.name}")
            if member.name.startswith("/") or ".." in Path(member.name).parts:
                raise ValueError(f"release archive contains an unsafe path: {member.name}")
            stream = package.extractfile(member)
            if stream is None:
                raise ValueError(f"cannot read release member: {member.name}")
            path = destination / member.name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(stream.read())
            path.chmod(member.mode)
    return destination / "omegon"


def runtime_inspection(binary: Path) -> dict:
    with tempfile.TemporaryDirectory(prefix="omegon-budget-runtime-") as directory:
        root = Path(directory)
        executable = extract_runtime(binary, root) if binary.suffix == ".gz" else binary
        workspace = root / "workspace"
        home = root / "home"
        workspace.mkdir()
        home.mkdir()
        env = os.environ.copy()
        env["HOME"] = str(home)
        env["OMEGON_LOG"] = "error"
        result = subprocess.run(
            [str(executable), "composition-inspect", "--profile", "full", "--cwd", str(workspace)],
            cwd=workspace,
            env=env,
            capture_output=True,
            text=True,
            timeout=90,
        )
        if result.returncode != 0:
            raise ValueError(f"runtime composition inspection failed: {result.stderr.strip()}")
        return json.loads(result.stdout)


def lock_counts(archive: Path) -> tuple[int, int]:
    with tarfile.open(archive, "r:gz") as package:
        counts = []
        for executable in ("omegon-maintain", "omegon"):
            stream = package.extractfile(f"{executable}.composition-lock.json")
            if stream is None:
                raise ValueError(f"missing resident lock for {executable}")
            lock = json.load(stream)
            contributions = lock.get("contributions")
            if not isinstance(contributions, list):
                raise ValueError(f"malformed resident lock for {executable}")
            counts.append(len(contributions))
    return counts[0], counts[1]


def collect(binary_dir: Path, archive: Path, target: str) -> dict:
    inspection = runtime_inspection(archive)
    startup = inspection["startup_tasks"]
    schema = inspection["model_schema"]
    callable_capabilities = inspection["callable_capabilities"]
    if inspection.get("profile") != "full":
        raise ValueError("runtime budget inspection did not report the full profile")
    maintenance_resident, normal_resident = lock_counts(archive)
    return {
        "schema_version": 1,
        "target": target,
        "profiles": {
            "maintenance": {
                "metrics": {
                    "dependency_count": dependency_count("omegon-maintain", target),
                    "binary_size_bytes": (binary_dir / "omegon-maintain").stat().st_size,
                    "startup_task_count": 0,
                    "model_schema_tokens": 0,
                    "resident_capabilities": maintenance_resident,
                    "default_callable_capabilities": 0,
                },
                "owners": {},
            },
            "normal": {
                "metrics": {
                    "dependency_count": dependency_count("omegon", target),
                    "binary_size_bytes": (binary_dir / "omegon").stat().st_size,
                    "startup_task_count": startup["count"],
                    "model_schema_tokens": schema["count"],
                    "resident_capabilities": normal_resident,
                    "default_callable_capabilities": len(callable_capabilities),
                },
                "owners": {
                    "startup_tasks": startup["owners"],
                    "model_schema_tokens": schema["owners"],
                },
            },
        },
    }


def metric_policy(policy: dict, profile: str, metric: str, target: str) -> dict:
    approved = policy["profiles"][profile][metric]
    if "targets" in approved:
        try:
            return approved["targets"][target]
        except KeyError as error:
            raise ValueError(f"{profile}.{metric}: target has no approved budget: {target}") from error
    return approved


def validate_measurement(measurement: dict) -> None:
    if measurement.get("schema_version") != 1 or not isinstance(measurement.get("target"), str):
        raise ValueError("measurement schema or target is invalid")
    profiles = measurement.get("profiles")
    if not isinstance(profiles, dict) or set(profiles) != {"maintenance", "normal"}:
        raise ValueError("measurement must contain exactly maintenance and normal profiles")
    for profile, row in profiles.items():
        metrics = row.get("metrics")
        if not isinstance(metrics, dict) or set(metrics) != set(METRICS):
            raise ValueError(f"{profile}: measurement metric set is invalid")
        if any(not isinstance(metrics[name], int) or metrics[name] < 0 for name in METRICS):
            raise ValueError(f"{profile}: measurement metrics must be nonnegative integers")
        if not isinstance(row.get("owners"), dict):
            raise ValueError(f"{profile}: owner diagnostics are required")


def enforce(measurement: dict, policy: dict) -> list[str]:
    validate_measurement(measurement)
    target = measurement["target"]
    failures = []
    for profile in ("maintenance", "normal"):
        actuals = measurement["profiles"][profile]["metrics"]
        for metric in METRICS:
            approved = metric_policy(policy, profile, metric, target)
            if set(approved) != {"baseline", "max_delta"}:
                raise ValueError(f"{profile}.{metric}: budget must contain baseline and max_delta")
            baseline = approved["baseline"]
            max_delta = approved["max_delta"]
            if not isinstance(baseline, int) or not isinstance(max_delta, int) or baseline < 0 or max_delta < 0:
                raise ValueError(f"{profile}.{metric}: malformed budget")
            limit = baseline + max_delta
            actual = actuals[metric]
            print(f"{target}.{profile}.{metric}: actual={actual} baseline={baseline} delta={actual - baseline:+d} limit={limit}")
            if actual > limit:
                failures.append(f"{target}.{profile}.{metric}: {actual} > {limit}")
        for kind, owners in measurement["profiles"][profile]["owners"].items():
            if not isinstance(owners, dict) or any(not isinstance(value, int) or value < 0 for value in owners.values()):
                raise ValueError(f"{profile}: malformed owner diagnostics for {kind}")
            for owner, value in sorted(owners.items()):
                print(f"  {kind} owner={owner} count={value}")
    return failures


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary-dir", type=Path)
    parser.add_argument("--archive", type=Path)
    parser.add_argument("--target")
    parser.add_argument("--measurement", type=Path)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--policy", type=Path, default=POLICY)
    args = parser.parse_args()
    try:
        policy = json.loads(args.policy.read_text())
        if policy.get("schema_version") != 1:
            raise ValueError("budget policy schema is invalid")
        if args.measurement:
            measurement = json.loads(args.measurement.read_text())
        elif args.binary_dir and args.archive and args.target:
            measurement = collect(args.binary_dir, args.archive, args.target)
        else:
            raise ValueError("provide --measurement or --binary-dir, --archive, and --target")
        if args.output:
            args.output.write_text(json.dumps(measurement, indent=2, sort_keys=True) + "\n")
        failures = enforce(measurement, policy)
    except (OSError, ValueError, KeyError, json.JSONDecodeError, subprocess.SubprocessError, tarfile.TarError) as error:
        print(f"composition budget collection failed: {error}")
        return 1
    if failures:
        print("composition budgets exceeded:\n" + "\n".join(failures))
        return 1
    print("composition budgets passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
