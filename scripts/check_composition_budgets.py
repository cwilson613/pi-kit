#!/usr/bin/env python3
"""Measure release and additive artifact compositions and enforce budgets."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import subprocess
import tarfile
import tempfile
from collections import Counter
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
ARTIFACT_ROWS = ("kernel-only", "kernel+codescan", "full-product")
ARTIFACT_METRICS = (
    "host_binary_size_bytes",
    "sidecar_binary_size_bytes",
    "aggregate_installed_size_bytes",
    "dependency_count",
    "startup_task_count",
    "external_process_count",
    "model_schema_tokens",
    "resident_capability_count",
    "callable_capability_count",
)
HOST_OWNER = "omegon-host"
SIDECAR_OWNER = "omegon-codescan"
TARGETED_ARTIFACT_METRICS = {
    "host_binary_size_bytes",
    "sidecar_binary_size_bytes",
    "aggregate_installed_size_bytes",
}


def _cargo_package_count(command: list[str]) -> int:
    result = subprocess.run(
        command,
        cwd=ROOT,
        capture_output=True,
        text=True,
        check=True,
    )
    packages = {
        line.split()[0]
        for line in result.stdout.splitlines()
        if line and not line.startswith("[")
    }
    return max(0, len(packages) - 1)


def _digest(path: Path) -> str:
    value = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            value.update(block)
    return value.hexdigest()


def dependency_count(package: str, target: str) -> int:
    return _cargo_package_count(
        [
            "cargo",
            "tree",
            "-p",
            package,
            "--target",
            target,
            "--prefix",
            "none",
            "--edges",
            "normal",
        ]
    )


def artifact_dependency_count(kind: str, target: str) -> int:
    command = ["cargo", "tree", "--target", target, "--prefix", "none", "--edges", "normal"]
    if kind == "kernel":
        command[2:2] = ["-p", "omegon"]
        command.extend(["--no-default-features", "--features", "kernel-host"])
    elif kind == "full-product":
        command[2:2] = ["-p", "omegon"]
    elif kind == "codescan":
        command[2:2] = [
            "--manifest-path",
            "extensions/omegon-codescan/Cargo.toml",
        ]
    else:
        raise ValueError(f"unknown artifact dependency graph: {kind}")
    return _cargo_package_count(command)


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


def _counted_runtime(inspection: dict, field: str, row: str) -> tuple[int, dict[str, int]]:
    value = inspection.get(field)
    if not isinstance(value, dict) or set(value) != {"count", "owners"}:
        raise ValueError(f"{row}: malformed runtime measurement {field}")
    count = value["count"]
    owners = value["owners"]
    if (
        not isinstance(count, int)
        or count < 0
        or not isinstance(owners, dict)
        or any(not isinstance(owner, str) or not owner for owner in owners)
        or any(not isinstance(amount, int) or amount <= 0 for amount in owners.values())
        or sum(owners.values()) != count
    ):
        raise ValueError(f"{row}: malformed runtime measurement {field}")
    return count, owners


def _identity_owners(inspection: dict, field: str, row: str) -> tuple[int, dict[str, int]]:
    values = inspection.get(field)
    if (
        not isinstance(values, list)
        or any(not isinstance(value, str) or not value for value in values)
        or len(values) != len(set(values))
    ):
        raise ValueError(f"{row}: malformed runtime inventory {field}")
    return len(values), {value: 1 for value in values}


def _process_owners(inspection: dict, row: str) -> tuple[int, dict[str, int]]:
    processes = inspection.get("external_processes")
    if not isinstance(processes, list):
        raise ValueError(f"{row}: malformed runtime process inventory")
    owners: Counter[str] = Counter()
    for process in processes:
        if not isinstance(process, dict) or not isinstance(process.get("owner"), str):
            raise ValueError(f"{row}: malformed runtime process inventory")
        owners[process["owner"]] += 1
    return len(processes), dict(owners)


def _installed_owners(root: Path) -> tuple[int, dict[str, int]]:
    owners: Counter[str] = Counter()
    for path in root.rglob("*"):
        if not path.is_file():
            continue
        relative = path.relative_to(root)
        owner = (
            SIDECAR_OWNER
            if relative.parts[:4] == ("share", "omegon", "extensions", SIDECAR_OWNER)
            or relative.parts == ("share", "omegon", "components", "core-codescan.lock.json")
            else HOST_OWNER
        )
        owners[owner] += path.stat().st_size
    return sum(owners.values()), dict(owners)


def collect_artifacts(evidence: dict) -> dict:
    if (
        evidence.get("schema_version") != 1
        or not isinstance(evidence.get("target"), str)
        or not isinstance(evidence.get("cargo_profile"), str)
        or not evidence["cargo_profile"]
        or set(evidence.get("artifact_rows", {})) != set(ARTIFACT_ROWS)
    ):
        raise ValueError("artifact ladder evidence is invalid")
    target = evidence["target"]
    host_dependencies = {
        "kernel": artifact_dependency_count("kernel", target),
        "full-product": artifact_dependency_count("full-product", target),
    }
    sidecar_dependencies = artifact_dependency_count("codescan", target)
    kernel_host = Path(evidence["artifact_rows"]["kernel-only"]["host_binary"])
    additive_host = Path(evidence["artifact_rows"]["kernel+codescan"]["host_binary"])
    if (
        not kernel_host.is_file()
        or not additive_host.is_file()
        or _digest(kernel_host) != _digest(additive_host)
    ):
        raise ValueError("kernel+codescan: host bytes differ from kernel-only")
    measured = {}
    for name in ARTIFACT_ROWS:
        source = evidence["artifact_rows"][name]
        install = Path(source["install_root"])
        host = Path(source["host_binary"])
        sidecar_value = source.get("sidecar_binary")
        sidecar = Path(sidecar_value) if sidecar_value is not None else None
        inspection = source.get("inspection")
        if not install.is_dir() or not host.is_file() or not isinstance(inspection, dict):
            raise ValueError(f"{name}: artifact evidence paths or inspection are invalid")
        if name == "kernel-only" and sidecar is not None:
            raise ValueError("kernel-only: sidecar must be absent")
        if name != "kernel-only" and (sidecar is None or not sidecar.is_file()):
            raise ValueError(f"{name}: declared codescan sidecar is absent")

        startup_count, startup_owners = _counted_runtime(inspection, "startup_tasks", name)
        schema_count, schema_owners = _counted_runtime(inspection, "model_schema", name)
        resident_count, resident_owners = _identity_owners(
            inspection, "resident_capabilities", name
        )
        callable_count, callable_owners = _identity_owners(
            inspection, "callable_capabilities", name
        )
        process_count, process_owners = _process_owners(inspection, name)
        installed_size, installed_owners = _installed_owners(install)
        host_dependency_count = host_dependencies[
            "full-product" if name == "full-product" else "kernel"
        ]
        dependency_owners = {HOST_OWNER: host_dependency_count}
        if sidecar is not None:
            dependency_owners[SIDECAR_OWNER] = sidecar_dependencies
        metrics = {
            "host_binary_size_bytes": host.stat().st_size,
            "sidecar_binary_size_bytes": 0 if sidecar is None else sidecar.stat().st_size,
            "aggregate_installed_size_bytes": installed_size,
            "dependency_count": sum(dependency_owners.values()),
            "startup_task_count": startup_count,
            "external_process_count": process_count,
            "model_schema_tokens": schema_count,
            "resident_capability_count": resident_count,
            "callable_capability_count": callable_count,
        }
        measured[name] = {
            "metrics": metrics,
            "owners": {
                "host_binary_size_bytes": {HOST_OWNER: metrics["host_binary_size_bytes"]},
                "sidecar_binary_size_bytes": (
                    {}
                    if sidecar is None
                    else {SIDECAR_OWNER: metrics["sidecar_binary_size_bytes"]}
                ),
                "aggregate_installed_size_bytes": installed_owners,
                "dependency_count": dependency_owners,
                "startup_task_count": startup_owners,
                "external_process_count": process_owners,
                "model_schema_tokens": schema_owners,
                "resident_capability_count": resident_owners,
                "callable_capability_count": callable_owners,
            },
        }
    measurement = {
        "schema_version": 1,
        "target": target,
        "cargo_profile": evidence["cargo_profile"],
        "artifact_rows": measured,
    }
    validate_artifact_measurement(measurement)
    return measurement


def metric_policy(policy: dict, profile: str, metric: str, target: str) -> dict:
    approved = policy["profiles"][profile][metric]
    if "targets" in approved:
        try:
            return approved["targets"][target]
        except KeyError as error:
            raise ValueError(f"{profile}.{metric}: target has no approved budget: {target}") from error
    return approved


def artifact_metric_policy(policy: dict, row: str, metric: str, target: str) -> dict:
    approved = policy["artifact_rows"][row][metric]
    if "targets" in approved:
        try:
            return approved["targets"][target]
        except KeyError as error:
            raise ValueError(f"{row}.{metric}: target has no approved budget: {target}") from error
    return approved


def _validate_budget(approved: dict, label: str) -> tuple[int, int]:
    if not isinstance(approved, dict) or set(approved) != {"baseline", "max_delta"}:
        raise ValueError(f"{label}: budget must contain baseline and max_delta")
    baseline = approved["baseline"]
    max_delta = approved["max_delta"]
    if (
        not isinstance(baseline, int)
        or not isinstance(max_delta, int)
        or baseline < 0
        or max_delta < 0
    ):
        raise ValueError(f"{label}: malformed budget")
    return baseline, max_delta


def validate_policy(policy: dict, *, artifact_required: bool = True) -> None:
    if policy.get("schema_version") != 1:
        raise ValueError("budget policy schema is invalid")
    profiles = policy.get("profiles")
    rows = policy.get("artifact_rows")
    if not isinstance(profiles, dict) or set(profiles) != {"maintenance", "normal"}:
        raise ValueError("budget policy legacy profiles are invalid")
    if rows is None and not artifact_required:
        return
    if not isinstance(rows, dict) or set(rows) != set(ARTIFACT_ROWS):
        raise ValueError("budget policy artifact rows are invalid")
    target_policy = profiles["normal"].get("binary_size_bytes", {}).get("targets")
    if not isinstance(target_policy, dict) or not target_policy:
        raise ValueError("budget policy supported targets are invalid")
    supported_targets = set(target_policy)
    for row, metrics in rows.items():
        if not isinstance(metrics, dict) or set(metrics) != set(ARTIFACT_METRICS):
            raise ValueError(f"{row}: artifact budget metric set is invalid")
        for metric, approved in metrics.items():
            if metric in TARGETED_ARTIFACT_METRICS:
                targets = approved.get("targets") if isinstance(approved, dict) else None
                if not isinstance(targets, dict) or set(targets) != supported_targets:
                    raise ValueError(f"{row}.{metric}: supported target budgets are incomplete")
                for target, budget in targets.items():
                    _validate_budget(budget, f"{row}.{metric}.{target}")
            else:
                _validate_budget(approved, f"{row}.{metric}")


def validate_measurement(measurement: dict) -> None:
    if measurement.get("schema_version") != 1 or not isinstance(
        measurement.get("target"), str
    ):
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


def validate_artifact_measurement(measurement: dict) -> None:
    if measurement.get("schema_version") != 1 or not isinstance(
        measurement.get("target"), str
    ):
        raise ValueError("artifact measurement schema or target is invalid")
    rows = measurement.get("artifact_rows")
    if not isinstance(rows, dict) or set(rows) != set(ARTIFACT_ROWS):
        raise ValueError("measurement must contain exactly the additive artifact rows")
    for name, row in rows.items():
        metrics = row.get("metrics")
        owners = row.get("owners")
        if not isinstance(metrics, dict) or set(metrics) != set(ARTIFACT_METRICS):
            raise ValueError(f"{name}: artifact measurement metric set is invalid")
        if any(not isinstance(metrics[metric], int) or metrics[metric] < 0 for metric in ARTIFACT_METRICS):
            raise ValueError(f"{name}: artifact metrics must be nonnegative integers")
        if not isinstance(owners, dict) or set(owners) != set(ARTIFACT_METRICS):
            raise ValueError(f"{name}: owner diagnostics are required for every metric")
        for metric in ARTIFACT_METRICS:
            evidence = owners[metric]
            if (
                not isinstance(evidence, dict)
                or any(not isinstance(owner, str) or not owner for owner in evidence)
                or any(not isinstance(value, int) or value <= 0 for value in evidence.values())
                or sum(evidence.values()) != metrics[metric]
            ):
                raise ValueError(f"{name}: malformed owner diagnostics for {metric}")
        if owners["host_binary_size_bytes"] != {HOST_OWNER: metrics["host_binary_size_bytes"]}:
            raise ValueError(f"{name}: host binary owner diagnostics are invalid")
        expected_sidecar_owners = (
            {}
            if metrics["sidecar_binary_size_bytes"] == 0
            else {SIDECAR_OWNER: metrics["sidecar_binary_size_bytes"]}
        )
        if owners["sidecar_binary_size_bytes"] != expected_sidecar_owners:
            raise ValueError(f"{name}: sidecar binary owner diagnostics are invalid")
        if set(owners["aggregate_installed_size_bytes"]) - {HOST_OWNER, SIDECAR_OWNER}:
            raise ValueError(f"{name}: installed-size owner diagnostics are invalid")
    kernel = rows["kernel-only"]
    additive = rows["kernel+codescan"]
    if kernel["metrics"]["sidecar_binary_size_bytes"] != 0 or kernel["owners"]["sidecar_binary_size_bytes"]:
        raise ValueError("kernel-only: sidecar cost must be zero and owner evidence empty")
    if kernel["metrics"]["host_binary_size_bytes"] != additive["metrics"]["host_binary_size_bytes"]:
        raise ValueError("kernel+codescan: host bytes differ from kernel-only")
    unchanged = (
        "startup_task_count",
        "model_schema_tokens",
        "resident_capability_count",
        "callable_capability_count",
    )
    for metric in unchanged:
        if kernel["metrics"][metric] != additive["metrics"][metric] or kernel["owners"][metric] != additive["owners"][metric]:
            raise ValueError(f"kernel+codescan: additive sidecar changed host metric {metric}")
    process_delta = additive["metrics"]["external_process_count"] - kernel["metrics"]["external_process_count"]
    if process_delta != additive["owners"]["external_process_count"].get(SIDECAR_OWNER, 0):
        raise ValueError("kernel+codescan: process delta is not owned by omegon-codescan")
    for metric in ("sidecar_binary_size_bytes", "aggregate_installed_size_bytes", "dependency_count"):
        delta = additive["metrics"][metric] - kernel["metrics"][metric]
        if delta != additive["owners"][metric].get(SIDECAR_OWNER, 0):
            raise ValueError(f"kernel+codescan: {metric} delta is not owned by omegon-codescan")


def enforce(measurement: dict, policy: dict) -> list[str]:
    validate_measurement(measurement)
    target = measurement["target"]
    failures = []
    for profile in ("maintenance", "normal"):
        actuals = measurement["profiles"][profile]["metrics"]
        for metric in METRICS:
            baseline, max_delta = _validate_budget(
                metric_policy(policy, profile, metric, target), f"{profile}.{metric}"
            )
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


def enforce_artifacts(measurement: dict, policy: dict) -> list[str]:
    validate_artifact_measurement(measurement)
    target = measurement["target"]
    failures = []
    for row in ARTIFACT_ROWS:
        actuals = measurement["artifact_rows"][row]["metrics"]
        for metric in ARTIFACT_METRICS:
            baseline, max_delta = _validate_budget(
                artifact_metric_policy(policy, row, metric, target), f"{row}.{metric}"
            )
            limit = baseline + max_delta
            actual = actuals[metric]
            print(f"{target}.{row}.{metric}: actual={actual} baseline={baseline} delta={actual - baseline:+d} limit={limit}")
            for owner, value in sorted(measurement["artifact_rows"][row]["owners"][metric].items()):
                print(f"  owner={owner} count={value}")
            if actual > limit:
                failures.append(f"{target}.{row}.{metric}: {actual} > {limit}")
    return failures


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary-dir", type=Path)
    parser.add_argument("--archive", type=Path)
    parser.add_argument("--target")
    parser.add_argument("--measurement", type=Path)
    parser.add_argument("--artifact-evidence", type=Path)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--policy", type=Path, default=POLICY)
    args = parser.parse_args()
    try:
        policy = json.loads(args.policy.read_text())
        if args.artifact_evidence:
            measurement = collect_artifacts(json.loads(args.artifact_evidence.read_text()))
            artifact_mode = True
        elif args.measurement:
            measurement = json.loads(args.measurement.read_text())
            artifact_mode = "artifact_rows" in measurement
        elif args.binary_dir and args.archive and args.target:
            measurement = collect(args.binary_dir, args.archive, args.target)
            artifact_mode = False
        else:
            raise ValueError("provide --measurement, --artifact-evidence, or --binary-dir, --archive, and --target")
        validate_policy(policy, artifact_required=artifact_mode)
        if args.output:
            args.output.parent.mkdir(parents=True, exist_ok=True)
            args.output.write_text(json.dumps(measurement, indent=2, sort_keys=True) + "\n")
        if artifact_mode and measurement.get("cargo_profile") not in (None, "release"):
            validate_artifact_measurement(measurement)
            print(f"artifact budgets collected without release enforcement: cargo_profile={measurement['cargo_profile']}")
            return 0
        failures = enforce_artifacts(measurement, policy) if artifact_mode else enforce(measurement, policy)
    except (OSError, ValueError, KeyError, TypeError, json.JSONDecodeError, subprocess.SubprocessError, tarfile.TarError) as error:
        print(f"composition budget collection failed: {error}")
        return 1
    if failures:
        print("composition budgets exceeded:\n" + "\n".join(failures))
        return 1
    print("composition budgets passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
