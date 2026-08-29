#!/usr/bin/env python3
"""Build and exercise source, linked-development, and release compositions."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import subprocess
import sys
import tarfile
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import content_pack_manifest  # noqa: E402

ROOT = Path(__file__).resolve().parents[1]
POLICY = ROOT / "fixtures/release-composition-matrix-v1.json"
REQUIRED_PROFILES = {"maintenance", "interactive", "headless", "daemon", "full"}
REQUIRED_PATHS = {"source", "linked", "release"}
EXECUTABLES = {"omegon", "omegon-maintain"}
LOCKS = {f"{name}.composition-lock.json" for name in EXECUTABLES}
CONTENT_PREFIX = "share/omegon/content-packs/omegon-shipped/"


def load_policy(path: Path = POLICY) -> dict:
    policy = json.loads(path.read_text())
    if policy.get("schema_version") != 1 or set(policy.get("profiles", {})) != REQUIRED_PROFILES:
        raise ValueError("composition matrix must define exactly the five v1 profiles")
    for name, profile in policy["profiles"].items():
        if set(profile.get("paths", [])) != REQUIRED_PATHS:
            raise ValueError(f"{name}: all source, linked, and release paths must be explicit")
        if profile.get("executable") not in EXECUTABLES:
            raise ValueError(f"{name}: executable is not part of the companion pair")
        if not isinstance(profile.get("absent_optional"), list):
            raise ValueError(f"{name}: absent_optional inventory is required")
        if name != "maintenance" and (
            profile.get("artifact_profile") != "full-product"
            or profile.get("canonical_entrypoint") != ["omegon"]
            or not profile.get("runtime_mode")
            or not profile.get("surfaces")
        ):
            raise ValueError(f"{name}: full-product identity, runtime mode, and surfaces are required")
    return policy


def digest(path: Path) -> str:
    value = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            value.update(block)
    return value.hexdigest()


def run_json(
    command: list[str], profile: str, cwd: Path, env: dict[str, str], timeout: int = 90
) -> dict:
    result = subprocess.run(command, cwd=cwd, env=env, capture_output=True, text=True, timeout=timeout)
    if result.returncode != 0:
        detail = result.stderr.strip() or result.stdout.strip()
        raise ValueError(f"{profile}: probe failed: {detail}")
    try:
        return json.loads(result.stdout)
    except json.JSONDecodeError as error:
        raise ValueError(f"{profile}: probe did not emit one JSON document") from error


def validate_profile(payload: dict, profile: str, row: dict) -> None:
    if profile == "maintenance":
        if payload.get("composition", {}).get("profile") != "maintenance":
            raise ValueError("maintenance: identity did not report maintenance composition")
        return
    if payload.get("schema_version") != 1 or payload.get("profile") != profile:
        raise ValueError(f"{profile}: inspection identity is invalid")
    if payload.get("artifact_profile") != row["artifact_profile"]:
        raise ValueError(f"{profile}: artifact profile does not match the matrix")
    if payload.get("canonical_entrypoint") != row["canonical_entrypoint"]:
        raise ValueError(f"{profile}: canonical entrypoint does not match the matrix")
    if payload.get("runtime_mode") != row["runtime_mode"]:
        raise ValueError(f"{profile}: runtime mode does not match the matrix")
    if payload.get("surfaces") != row["surfaces"]:
        raise ValueError(f"{profile}: active surfaces do not match the matrix")
    if payload.get("absent_optional") != row["absent_optional"]:
        raise ValueError(f"{profile}: optional absence inventory does not match the matrix")
    for field in ("startup_tasks", "model_schema"):
        measurement = payload.get(field)
        if (
            not isinstance(measurement, dict)
            or not isinstance(measurement.get("count"), int)
            or measurement["count"] < 0
            or not isinstance(measurement.get("owners"), dict)
            or sum(measurement["owners"].values()) != measurement["count"]
        ):
            raise ValueError(f"{profile}: malformed runtime measurement {field}")
    callable_capabilities = payload.get("callable_capabilities")
    resident_capabilities = payload.get("resident_capabilities")
    if (
        not isinstance(callable_capabilities, list)
        or not callable_capabilities
        or len(set(callable_capabilities)) != len(callable_capabilities)
        or not all(value.startswith("tool:") for value in callable_capabilities)
        or resident_capabilities != row["resident_capabilities"]
    ):
        raise ValueError(f"{profile}: authoritative capability inventory is invalid")


def profile_arguments(profile: str) -> list[str]:
    if profile == "maintenance":
        return ["--json", "identity"]
    return ["composition-inspect", "--profile", profile]


def validate_resident_lock(
    path: Path,
    executable: Path,
    target: str,
    verification: str,
    workflow_identity: str,
) -> None:
    lock = json.loads(path.read_text())
    identity = executable.name
    expected = {
        "omegon": {
            "system:constitutional-kernel",
            "system:default-loop",
            "system:host-effects",
            "feature:codescan",
            "feature:context-compaction",
            "feature:git",
            "feature:lifecycle",
            "feature:memory",
        },
        "omegon-maintain": {"system:maintenance-kernel"},
    }[identity]
    contributions = lock.get("contributions")
    signing = lock.get("signing_identity")
    if (
        lock.get("schema_version") != 1
        or lock.get("executable_identity") != identity
        or lock.get("executable_digest") != digest(executable)
        or lock.get("target") != target
        or lock.get("protocol_minimum", 0) == 0
        or lock.get("protocol_minimum") > lock.get("protocol_maximum", 0)
        or not isinstance(contributions, list)
        or {entry.get("identity") for entry in contributions} != expected
        or len(contributions) != len(expected)
        or signing
        != {
            "issuer": "https://token.actions.githubusercontent.com",
            "verification": verification,
            "workflow_identity": workflow_identity,
        }
    ):
        raise ValueError(f"invalid exact resident lock: {path}")
    for entry in contributions:
        required = entry["identity"].startswith("system:")
        if (
            entry.get("artifact_path") != identity
            or entry.get("artifact_digest") != lock["executable_digest"]
            or entry.get("targets") != [target]
            or entry.get("required") is not required
            or entry.get("fallback") != ("fail_closed" if required else "typed_unavailable")
            or entry.get("state") != ("resident" if required else "resident_optional")
            or entry.get("protocol_minimum", 0) < lock["protocol_minimum"]
            or entry.get("protocol_maximum", 0) > lock["protocol_maximum"]
            or entry.get("protocol_minimum", 0) > entry.get("protocol_maximum", 0)
        ):
            raise ValueError(f"invalid resident contribution {entry.get('identity')}: {path}")


def validate_content_pack(root: Path) -> None:
    pack = root / CONTENT_PREFIX
    expected = {"content-pack.toml": content_pack_manifest.render().encode()}
    expected.update(
        {
            str(asset["path"]): (content_pack_manifest.ROOT / str(asset["path"])).read_bytes()
            for asset in content_pack_manifest.assets()
        }
    )
    for relative, contents in expected.items():
        path = pack / relative
        if not path.is_file() or path.read_bytes() != contents:
            raise ValueError(f"linked/release content asset is missing or stale: {path}")


def verify_archive_inventory(archive: Path, target: str) -> Path:
    directory = Path(tempfile.mkdtemp(prefix="omegon-composition-release-"))
    with tarfile.open(archive, "r:gz") as package:
        members = [member for member in package if member.isfile()]
        names = {member.name for member in members}
        required = EXECUTABLES | LOCKS | {f"{CONTENT_PREFIX}content-pack.toml"}
        if missing := sorted(required - names):
            raise ValueError(f"release archive lacks required composition members: {missing}")
        for member in members:
            if member.name not in EXECUTABLES | LOCKS and not member.name.startswith(CONTENT_PREFIX):
                raise ValueError(f"release archive has an unexpected member: {member.name}")
            stream = package.extractfile(member)
            if stream is None:
                raise ValueError(f"cannot read archive member: {member.name}")
            destination = directory / member.name
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_bytes(stream.read())
            destination.chmod(member.mode)
    suffix = f"-{target}.tar.gz"
    if not archive.name.startswith("omegon-") or not archive.name.endswith(suffix):
        raise ValueError("release archive filename does not match its target")
    version = archive.name[len("omegon-") : -len(suffix)]
    workflow_identity = (
        "https://github.com/styrene-lab/omegon/.github/workflows/release.yml@"
        f"refs/tags/v{version}"
    )
    for executable_name in EXECUTABLES:
        validate_resident_lock(
            directory / f"{executable_name}.composition-lock.json",
            directory / executable_name,
            target,
            "required",
            workflow_identity,
        )
    validate_content_pack(directory)
    return directory


def validate_linked_install(home: Path, binary_dir: Path, target: str) -> dict[str, Path]:
    launchers = {name: home / ".local/bin" / name for name in ("omegon", "om", "omegon-maintain")}
    for name, launcher in launchers.items():
        if not launcher.is_file() or not os.access(launcher, os.X_OK):
            raise ValueError(f"linked launcher is missing or not executable: {launcher}")
        if launcher.resolve() in {path.resolve() for path in binary_dir.iterdir() if path.is_file()}:
            raise ValueError(f"linked path must execute a launcher, not a target binary: {name}")
    lock_root = home / ".omegon/share/omegon/composition"
    for executable_name in EXECUTABLES:
        validate_resident_lock(
            lock_root / f"{executable_name}.composition-lock.json",
            binary_dir / executable_name,
            target,
            "not_applicable",
            "local:source-or-linked-build",
        )
    validate_content_pack(home / ".omegon")
    return {"omegon": launchers["omegon"], "omegon-maintain": launchers["omegon-maintain"]}


def source_command(profile: str, row: dict, cargo_profile: str, workspace: Path) -> list[str]:
    return [
        "cargo",
        "run",
        "--quiet",
        "--locked",
        "--profile",
        cargo_profile,
        "-p",
        row["executable"],
        "--",
        *profile_arguments(profile),
        *( ["--cwd", str(workspace)] if profile != "maintenance" else [] ),
    ]


def exercise(
    path: str,
    policy: dict,
    *,
    binary_dir: Path | None,
    archive: Path | None,
    linked_home: Path | None,
    target: str | None,
    cargo_profile: str,
) -> None:
    if path not in REQUIRED_PATHS:
        raise ValueError(f"unknown packaging path: {path}")
    with tempfile.TemporaryDirectory(prefix="omegon-composition-workspace-") as workspace_name:
        workspace = Path(workspace_name)
        env = os.environ.copy()
        env["OMEGON_LOG"] = "error"
        if path == "source":
            commands = {
                profile: source_command(profile, row, cargo_profile, workspace)
                for profile, row in policy["profiles"].items()
            }
            command_cwd = ROOT
        elif path == "linked":
            if binary_dir is None or linked_home is None or target is None:
                raise ValueError("linked path requires --binary-dir, --linked-home, and --target")
            executables = validate_linked_install(linked_home, binary_dir, target)
            env["HOME"] = str(linked_home)
            env["OMEGON_CHANNEL"] = "composition-ci"
            commands = {
                profile: [
                    str(executables[row["executable"]]),
                    *profile_arguments(profile),
                    *(["--cwd", str(workspace)] if profile != "maintenance" else []),
                ]
                for profile, row in policy["profiles"].items()
            }
            command_cwd = workspace
            for executable_name, launcher in executables.items():
                result = subprocess.run(
                    [str(launcher), "--which"],
                    cwd=workspace,
                    env=env,
                    capture_output=True,
                    text=True,
                    timeout=30,
                    check=True,
                )
                expected_target = (binary_dir / executable_name).resolve()
                if "reason: channel:composition-ci" not in result.stdout or f"target: {expected_target}" not in result.stdout:
                    raise ValueError(f"linked launcher did not resolve the installed channel: {launcher}")
        else:
            if archive is None or target is None:
                raise ValueError("release path requires --archive and --target")
            extracted = verify_archive_inventory(archive, target)
            commands = {
                profile: [
                    str(extracted / row["executable"]),
                    *profile_arguments(profile),
                    *(["--cwd", str(workspace)] if profile != "maintenance" else []),
                ]
                for profile, row in policy["profiles"].items()
            }
            command_cwd = workspace
        for profile, row in policy["profiles"].items():
            timeout = 900 if path == "source" else 90
            validate_profile(
                run_json(commands[profile], profile, command_cwd, env, timeout), profile, row
            )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--path", choices=sorted(REQUIRED_PATHS), required=True)
    parser.add_argument("--binary-dir", type=Path)
    parser.add_argument("--archive", type=Path)
    parser.add_argument("--linked-home", type=Path)
    parser.add_argument("--target")
    parser.add_argument("--cargo-profile", default="release")
    parser.add_argument("--policy", type=Path, default=POLICY)
    args = parser.parse_args()
    try:
        policy = load_policy(args.policy)
        exercise(
            args.path,
            policy,
            binary_dir=args.binary_dir,
            archive=args.archive,
            linked_home=args.linked_home,
            target=args.target,
            cargo_profile=args.cargo_profile,
        )
    except (OSError, ValueError, KeyError, json.JSONDecodeError, subprocess.SubprocessError, tarfile.TarError) as error:
        print(f"composition matrix failed: {error}")
        return 1
    print(f"composition matrix passed: {args.path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
