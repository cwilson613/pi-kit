#!/usr/bin/env python3
"""Build and exercise source, linked-development, and release compositions."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import subprocess
import sys
import tarfile
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import content_pack_manifest  # noqa: E402
import package_release  # noqa: E402
import release_manifest  # noqa: E402

ROOT = Path(__file__).resolve().parents[1]
POLICY = ROOT / "fixtures/release-composition-matrix-v1.json"
REQUIRED_PROFILES = {"maintenance", "interactive", "headless", "daemon", "full"}
REQUIRED_ARTIFACT_ROWS = {"kernel-only", "kernel+codescan", "full-product"}
REQUIRED_PATHS = {"source", "linked", "release"}
EXECUTABLES = {"omegon", "omegon-maintain"}
LOCKS = {f"{name}.composition-lock.json" for name in EXECUTABLES}
CONTENT_PREFIX = "share/omegon/content-packs/omegon-shipped/"


def load_policy(path: Path = POLICY) -> dict:
    policy = json.loads(path.read_text())
    if policy.get("schema_version") != 1 or set(policy.get("profiles", {})) != REQUIRED_PROFILES:
        raise ValueError("composition matrix must define exactly the five v1 profiles")
    validate_artifact_rows(policy.get("artifact_rows"))
    validate_extracted_domains(policy.get("extracted_domains"), policy["artifact_rows"])
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


def validate_artifact_rows(rows: object) -> None:
    if not isinstance(rows, dict) or not REQUIRED_ARTIFACT_ROWS.issubset(rows):
        raise ValueError("composition matrix must define the required v1 artifact ladder")
    identities = [row.get("installation_identity") for row in rows.values()]
    if any(not isinstance(identity, str) or not identity for identity in identities):
        raise ValueError("every artifact row requires an installation identity")
    if len(set(identities)) != len(identities):
        raise ValueError("artifact-row installation identity must be unique")
    for name, row in rows.items():
        if set(row.get("paths", [])) != REQUIRED_PATHS:
            raise ValueError(f"{name}: all source, linked, and release paths must be explicit")
        for field in ("extensions", "restores", "typed_unavailable"):
            inventory = row.get(field)
            if (
                not isinstance(inventory, list)
                or any(not isinstance(value, str) or not value for value in inventory)
                or len(inventory) != len(set(inventory))
            ):
                raise ValueError(f"{name}: {field} inventory is required")
        if not isinstance(row.get("probe"), str) or not row["probe"]:
            raise ValueError(f"{name}: representative probe is required")

    kernel = rows["kernel-only"]
    cargo = kernel.get("cargo")
    boundary = kernel.get("positive_boundary")
    expected_residents = [
        "system:constitutional-kernel",
        "system:default-loop",
        "system:host-effects",
        "feature:codescan-adapter",
    ]
    if (
        kernel.get("artifact_profile") != "kernel-host-v1"
        or not isinstance(cargo, dict)
        or cargo.get("package") != "omegon"
        or cargo.get("bin") != "omegon-kernel-host"
        or cargo.get("default_features") is not False
        or cargo.get("features") != ["kernel-host"]
        or kernel.get("extensions") != []
        or kernel.get("restores") != []
        or not isinstance(boundary, dict)
        or not isinstance(boundary.get("dependency_roots"), list)
        or not boundary["dependency_roots"]
        or boundary["dependency_roots"] != sorted(set(boundary["dependency_roots"]))
        or boundary.get("resident_capabilities") != expected_residents
    ):
        raise ValueError("kernel-only: positive artifact boundary is invalid")

    additive = rows["kernel+codescan"]
    if (
        additive.get("artifact_profile") != kernel["artifact_profile"]
        or additive.get("host_artifact") != "kernel-only"
    ):
        raise ValueError("kernel+codescan must use the unchanged kernel-only host")

    product = rows["full-product"]
    product_cargo = product.get("cargo")
    if (
        product.get("artifact_profile") != "full-product"
        or not isinstance(product_cargo, dict)
        or product_cargo.get("package") != "omegon"
        or product_cargo.get("default_features") is not True
        or product_cargo.get("features") != []
    ):
        raise ValueError("full-product: compiled artifact declaration is invalid")


def validate_extracted_domains(domains: object, rows: dict) -> None:
    if not isinstance(domains, dict) or not domains:
        raise ValueError("composition matrix must declare extracted domains")

    services = []
    extensions = []
    roles = {
        "kernel_absence": ("typed_unavailable",),
        "additive_restoration": ("restores", "extensions"),
        "accumulated_product": ("restores", "extensions"),
    }
    for domain, declaration in domains.items():
        if not isinstance(domain, str) or not domain or not isinstance(declaration, dict):
            raise ValueError("extracted-domain declarations require a named object")
        service = declaration.get("service_identity")
        extension = declaration.get("extension_identity")
        if not isinstance(service, str) or not service.startswith("service:"):
            raise ValueError(f"{domain}: canonical service identity is required")
        if not isinstance(extension, str) or not extension:
            raise ValueError(f"{domain}: canonical extension identity is required")
        services.append(service)
        extensions.append(extension)

        selected_rows = []
        for role, evidence_fields in roles.items():
            assertion = declaration.get(role)
            if not isinstance(assertion, dict) or set(assertion) != {"row", "evidence"}:
                raise ValueError(f"{domain}: {role} row and evidence are required")
            row_name = assertion["row"]
            if not isinstance(row_name, str) or row_name not in rows:
                raise ValueError(f"{domain}: {role} references a missing artifact row")
            selected_rows.append(row_name)
            evidence = assertion["evidence"]
            if not isinstance(evidence, dict) or set(evidence) != set(evidence_fields):
                raise ValueError(f"{domain}: {role} evidence inventory is invalid")
            expected = {
                field: extension if field == "extensions" else service
                for field in evidence_fields
            }
            if evidence != expected:
                raise ValueError(f"{domain}: {role} service or extension identity is mismatched")
            for field, identity in evidence.items():
                if identity not in rows[row_name][field]:
                    raise ValueError(f"{domain}: {role} evidence is absent from {row_name}")

        if len(selected_rows) != len(set(selected_rows)):
            raise ValueError(f"{domain}: absence, restoration, and product rows must not alias")
        additive = rows[declaration["additive_restoration"]["row"]]
        if service in additive["typed_unavailable"]:
            raise ValueError(f"{domain}: service absence remains in the additive row")

    if len(services) != len(set(services)) or len(extensions) != len(set(extensions)):
        raise ValueError("extracted domains must not alias service or extension identities")


def digest(path: Path) -> str:
    value = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            value.update(block)
    return value.hexdigest()


def dynamic_source_digest(root: Path) -> str:
    value = hashlib.sha256()

    def hash_directory(directory: Path, relative: Path) -> None:
        for path in sorted(directory.iterdir(), key=lambda entry: entry.name):
            child = relative / path.name
            if path.is_symlink():
                raise ValueError(f"dynamic contribution source contains symlink: {path}")
            if path.is_dir():
                hash_directory(path, child)
                continue
            if not path.is_file():
                raise ValueError(f"dynamic contribution source contains unsupported entry: {path}")
            value.update(b"file\0")
            value.update(os.fsencode(child))
            value.update(b"\0")
            value.update(path.read_bytes())

    hash_directory(root, Path())
    return f"sha256:{value.hexdigest()}"


def process_is_running(pid: int) -> bool:
    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        return False
    except PermissionError:
        return True
    return True


def process_group_is_running(process_group: int) -> bool:
    try:
        os.kill(-process_group, 0)
    except ProcessLookupError:
        return False
    except PermissionError:
        return True
    return True


def host_target() -> str:
    result = subprocess.run(
        ["rustc", "-vV"], cwd=ROOT, capture_output=True, text=True, check=True
    )
    for line in result.stdout.splitlines():
        if line.startswith("host: "):
            return line.removeprefix("host: ")
    raise ValueError("rustc did not report its host target")


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
        "omegon": set(package_release.OMEGON_REQUIRED_RESIDENT_IDENTITIES)
        | set(package_release.OMEGON_OPTIONAL_RESIDENT_IDENTITIES),
        "omegon-maintain": set(package_release.OMEGON_MAINTAIN_RESIDENT_IDENTITIES),
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
        members = list(package)
        if invalid := sorted(member.name for member in members if not member.isfile()):
            raise ValueError(f"release archive has a non-file member: {invalid}")
        member_names = [member.name for member in members]
        names = set(member_names)
        extension_names = [
            name for name in member_names if name.startswith(package_release.CODESCAN_PREFIX)
        ]
        required_extensions = set(package_release.CODESCAN_MEMBERS)
        if duplicates := sorted(
            name for name in required_extensions if member_names.count(name) > 1
        ):
            raise ValueError(f"release archive has a duplicate extension member: {duplicates}")
        unexpected_extensions = sorted(set(extension_names) - required_extensions)
        expected_basenames = {Path(name).name for name in required_extensions}
        if misplaced := [
            name for name in unexpected_extensions if Path(name).name in expected_basenames
        ]:
            raise ValueError(f"release archive has a misplaced extension member: {misplaced}")
        if unexpected_extensions:
            raise ValueError(
                f"release archive has an unexpected extension member: {unexpected_extensions}"
            )
        if missing := sorted(required_extensions - names):
            raise ValueError(f"release archive lacks required extension members: {missing}")
        required = (
            EXECUTABLES
            | LOCKS
            | {f"{CONTENT_PREFIX}content-pack.toml"}
            | required_extensions
        )
        if missing := sorted(required - names):
            raise ValueError(f"release archive lacks required composition members: {missing}")
        if len(member_names) != len(names):
            raise ValueError("release archive has a duplicate member")
        for member in members:
            if (
                member.name not in EXECUTABLES | LOCKS | required_extensions
                and not member.name.startswith(CONTENT_PREFIX)
            ):
                raise ValueError(f"release archive has an unexpected member: {member.name}")
            expected_mode = (
                0o755
                if member.name in EXECUTABLES | {package_release.CODESCAN_EXECUTABLE}
                else 0o644
            )
            if member.mode != expected_mode:
                raise ValueError(
                    f"release archive member {member.name} has mode {member.mode:04o}, expected {expected_mode:04o}"
                )
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
    validate_product_component_lock(
        json.loads((directory / package_release.CODESCAN_COMPONENT_LOCK).read_text()),
        directory,
        target,
    )
    return directory


def validate_product_component_lock(lock: dict, root: Path, target: str) -> None:
    expected = {
        "schema_version": 1,
        "component_id": "core:codescan",
        "wire_manifest_id": "omegon-codescan",
        "manifest_path": package_release.CODESCAN_MANIFEST,
        "executable_path": package_release.CODESCAN_EXECUTABLE,
        "target": target,
        "protocol_minimum": 1,
        "protocol_maximum": 1,
        "protocol_version": 1,
        "fallback": "typed_unavailable",
    }
    if any(lock.get(key) != value for key, value in expected.items()):
        raise ValueError("release component lock has invalid identity, target, protocol, or fallback")
    for path_field, digest_field in (
        ("manifest_path", "manifest_digest"),
        ("executable_path", "executable_digest"),
    ):
        payload = (root / lock[path_field]).read_bytes()
        if hashlib.sha256(payload).hexdigest() != lock.get(digest_field):
            raise ValueError(f"release component lock has substituted {path_field}")
    signing = lock.get("signing_identity")
    if not isinstance(signing, dict) or signing.get("issuer") != package_release.ISSUER:
        raise ValueError("release component lock has invalid signing authority")
    if signing.get("verification") != "required" or not str(
        signing.get("workflow_identity", "")
    ).startswith(
        "https://github.com/styrene-lab/omegon/.github/workflows/release.yml@refs/tags/v"
    ):
        raise ValueError("release component lock has invalid workflow authority")


def package_evidence(archive: Path, extracted: Path, target: str) -> dict:
    version = archive.name.removeprefix("omegon-").removesuffix(f"-{target}.tar.gz")
    manifest = release_manifest.build_package_manifest(
        archive=archive,
        tag=f"v{version}",
        target=target,
        repo="styrene-lab/omegon",
        commit="0" * 40,
    )
    return {
        "inventory": [member["path"] for member in manifest["members"]],
        "member_digests": {
            member["path"]: member["digest"] for member in manifest["members"]
        },
        "resident_locks": {
            name: json.loads((extracted / name).read_text()) for name in sorted(LOCKS)
        },
        "package_manifest": manifest,
    }


def archive_without_codescan(archive: Path, output: Path) -> None:
    with tarfile.open(archive, "r:gz") as source, tarfile.open(output, "w:gz") as destination:
        for member in source:
            if member.name == package_release.CODESCAN_EXECUTABLE:
                continue
            stream = source.extractfile(member)
            if stream is None:
                raise ValueError(f"cannot read archive member: {member.name}")
            destination.addfile(member, stream)


def replace_current_generation(current: Path, generation: Path) -> None:
    replacement = current.with_name(f".{current.name}.replacement")
    replacement.unlink(missing_ok=True)
    replacement.symlink_to(generation, target_is_directory=True)
    os.replace(replacement, current)


def exercise_installed_full_product(
    archive: Path,
    target: str,
    profile_row: dict,
    installed_generation: Path | None = None,
    executable_relative: Path = Path("omegon"),
) -> dict:
    """Exercise default and denied policy against one real extracted package."""
    with tempfile.TemporaryDirectory(prefix="omegon-installed-full-product-") as directory_name:
        root = Path(directory_name)
        generations = root / "versions"
        generations.mkdir()
        evidence_root = verify_archive_inventory(archive, target)
        default_evidence = package_evidence(archive, evidence_root, target)
        denied_evidence = package_evidence(archive, evidence_root, target)
        source_generation = installed_generation or evidence_root
        default_generation = Path(
            shutil.copytree(source_generation, generations / "default", symlinks=True)
        )
        denied_generation = Path(
            shutil.copytree(source_generation, generations / "denied", symlinks=True)
        )
        unavailable_generation = Path(
            shutil.copytree(source_generation, generations / "unavailable", symlinks=True)
        )
        installed_component = json.loads(
            (default_generation / package_release.CODESCAN_COMPONENT_LOCK).read_text()
        )
        expected_source_digest = dynamic_source_digest(
            (default_generation / package_release.CODESCAN_MANIFEST).parent
        )
        shutil.rmtree(evidence_root)
        if default_evidence != denied_evidence:
            raise ValueError("component policy changed installed package evidence")

        current = root / "current"
        current.symlink_to(default_generation, target_is_directory=True)
        default_workspace = root / "default-workspace"
        denied_workspace = root / "denied-workspace"
        for workspace in (default_workspace, denied_workspace):
            workspace.mkdir()
            (workspace / "codescan_probe.rs").write_text(
                "pub fn omegon_composition_codescan_probe() -> bool { true }\n"
            )

        default_home = root / "default-home"
        default_omegon_home = default_home / ".omegon"
        default_omegon_home.mkdir(parents=True)
        default_env = os.environ.copy()
        default_env.update(
            {
                "HOME": str(default_home),
                "OMEGON_HOME": str(default_omegon_home),
                "OMEGON_LOG": "error",
            }
        )
        def command(workspace: Path) -> list[str]:
            return [
                str(current / executable_relative),
                "composition-inspect",
                "--profile",
                "full",
                "--probe",
                "codescan-search",
                "--cwd",
                str(workspace),
            ]

        default = run_json(
            command(default_workspace),
            "installed-full-product-default",
            default_workspace,
            default_env,
            180,
        )
        validate_profile(default, "full", profile_row)
        default_probe = default.get("functional_probe", {})
        provenance = default_probe.get("service_provenance", {})
        processes = default.get("external_processes")
        if (
            default_probe.get("status") != "ok"
            or default_probe.get("component_id") != "core:codescan"
            or default_probe.get("wire_manifest_id") != "omegon-codescan"
            or default_probe.get("service_id") != "service:codescan"
            or default_probe.get("protocol_version") != 1
            or provenance.get("extension") != "omegon-codescan"
            or provenance.get("source_digest") != expected_source_digest
            or not isinstance(provenance.get("pid"), int)
            or not isinstance(processes, list)
            or len(processes) != 1
            or processes[0].get("owner") != "omegon-codescan"
            or processes[0].get("state") != "healthy"
            or processes[0].get("pid") != provenance["pid"]
            or process_is_running(provenance["pid"])
            or process_group_is_running(provenance["pid"])
        ):
            raise ValueError("installed default policy did not run and settle packaged codescan")

        replace_current_generation(current, unavailable_generation)
        (unavailable_generation / package_release.CODESCAN_EXECUTABLE).write_bytes(b"substituted")
        unavailable_home = root / "unavailable-home"
        unavailable_omegon_home = unavailable_home / ".omegon"
        unavailable_omegon_home.mkdir(parents=True)
        unavailable_env = os.environ.copy()
        unavailable_env.update(
            {
                "HOME": str(unavailable_home),
                "OMEGON_HOME": str(unavailable_omegon_home),
                "OMEGON_LOG": "error",
            }
        )
        unavailable = run_json(
            command(default_workspace),
            "installed-full-product-unavailable",
            default_workspace,
            unavailable_env,
            180,
        )
        unavailable_probe = unavailable.get("functional_probe", {})
        if (
            unavailable_probe.get("status") != "unavailable"
            or unavailable_probe.get("code") != "service:unavailable"
            or unavailable_probe.get("component_id") != "core:codescan"
            or unavailable.get("external_processes") != []
        ):
            raise ValueError("substituted packaged codescan was not typed locally unavailable")

        replace_current_generation(current, denied_generation)
        denied_home = root / "denied-home"
        denied_omegon_home = denied_home / ".omegon"
        denied_omegon_home.mkdir(parents=True)
        policy_path = denied_omegon_home / "component-policy.json"
        policy_path.write_text(
            '{"schemaVersion":1,"components":{"core:codescan":{"enabled":false}}}\n'
        )
        denied_env = os.environ.copy()
        denied_env.update(
            {
                "HOME": str(denied_home),
                "OMEGON_HOME": str(denied_omegon_home),
                "OMEGON_LOG": "error",
            }
        )
        denied = run_json(
            command(denied_workspace),
            "installed-full-product-denied",
            denied_workspace,
            denied_env,
            180,
        )
        validate_profile(denied, "full", profile_row)
        denied_probe = denied.get("functional_probe", {})
        expected_source = {"kind": "user-local", "path": str(policy_path)}
        if (
            denied_probe.get("code") != "service:disabled"
            or denied_probe.get("component_id") != "core:codescan"
            or denied_probe.get("determining_policy_source") != expected_source
            or denied.get("external_processes") != []
            or "tool:codebase_index" in denied.get("callable_capabilities", [])
            or "tool:codebase_search" in denied.get("callable_capabilities", [])
        ):
            raise ValueError("installed deny policy did not suppress packaged codescan before spawn")
        if any(path.name.startswith("codescan.db") for path in denied_workspace.rglob("*")):
            raise ValueError("installed deny policy allowed codescan workspace mutation")

        replace_current_generation(current, default_generation)
        rollback_valid = current.resolve() == default_generation.resolve()
        try:
            validate_product_component_lock(installed_component, current, target)
        except (OSError, ValueError):
            rollback_valid = False

        missing_archive = root / archive.name
        archive_without_codescan(archive, missing_archive)
        missing_codescan_rejected = False
        try:
            verify_archive_inventory(missing_archive, target)
        except ValueError as error:
            missing_codescan_rejected = "required extension members" in str(error)
        if not missing_codescan_rejected:
            raise ValueError("deny policy excused missing required codescan package content")

        return {
            "default": {
                **default_evidence,
                "probe": default_probe,
                "callable_capabilities": default["callable_capabilities"],
                "external_processes": processes,
            },
            "denied": {
                **denied_evidence,
                "probe": denied_probe,
                "callable_capabilities": denied["callable_capabilities"],
                "external_processes": denied["external_processes"],
            },
            "unavailable": {
                "probe": unavailable_probe,
                "external_processes": unavailable["external_processes"],
            },
            "rollback_valid": rollback_valid,
            "missing_codescan_rejected_under_deny": missing_codescan_rejected,
        }


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


def exercise_source_artifact_ladder(
    policy: dict, cargo_profile: str, budget_output: Path | None = None
) -> None:
    rows = policy["artifact_rows"]
    with tempfile.TemporaryDirectory(prefix="omegon-artifact-ladder-") as directory_name:
        root = Path(directory_name)
        target = ROOT / "target/composition/kernel-host"
        full_target = ROOT / "target/composition/full-product"
        subprocess.run(
            [sys.executable, "scripts/check_kernel_dependency_boundary.py"],
            cwd=ROOT,
            check=True,
        )
        subprocess.run(
            [
                "cargo",
                "build",
                "--locked",
                "--profile",
                cargo_profile,
                "-p",
                "omegon",
                "--bin",
                "omegon-kernel-host",
                "--no-default-features",
                "--features",
                "kernel-host",
                "--target-dir",
                str(target),
            ],
            cwd=ROOT,
            check=True,
        )
        subprocess.run(
            [
                "cargo",
                "build",
                "--release",
                "--locked",
                "--manifest-path",
                "extensions/omegon-codescan/Cargo.toml",
            ],
            cwd=ROOT,
            check=True,
        )
        subprocess.run(
            [
                "cargo",
                "build",
                "--locked",
                "--profile",
                cargo_profile,
                "-p",
                "omegon-maintain",
                "--target-dir",
                str(full_target),
            ],
            cwd=ROOT,
            check=True,
        )
        subprocess.run(
            [
                "cargo",
                "build",
                "--locked",
                "--profile",
                cargo_profile,
                "-p",
                "omegon",
                "--bin",
                "omegon",
                "--target-dir",
                str(full_target),
            ],
            cwd=ROOT,
            check=True,
        )
        output_profile = "debug" if cargo_profile == "dev" else cargo_profile
        kernel_binary = target / output_profile / "omegon-kernel-host"
        full_binary = full_target / output_profile / "omegon"
        if not kernel_binary.is_file():
            raise ValueError(f"kernel host artifact is missing: {kernel_binary}")
        if not full_binary.is_file():
            raise ValueError(f"full product artifact is missing: {full_binary}")
        maintain_binary = full_target / output_profile / "omegon-maintain"
        if not maintain_binary.is_file():
            raise ValueError(f"maintenance artifact is missing: {maintain_binary}")

        package_dir = root / "package-bin"
        package_dir.mkdir()
        shutil.copy2(full_binary, package_dir / "omegon")
        shutil.copy2(maintain_binary, package_dir / "omegon-maintain")
        target_name = host_target()
        archive = root / f"omegon-0.0.0-{target_name}.tar.gz"
        package_release.package(
            package_dir,
            archive,
            ROOT / "extensions/omegon-codescan/target/release/omegon-codescan",
            ROOT / "extensions/omegon-codescan/manifest.toml",
        )
        installed_acceptance = exercise_installed_full_product(
            archive, target_name, policy["profiles"]["full"]
        )

        installs = {
            "kernel-only": root / "install/kernel-only",
            "kernel+codescan": root / "install/kernel+codescan",
            "full-product": root / "install/full-product",
        }
        for install in installs.values():
            install.mkdir(parents=True)
        shutil.copy2(kernel_binary, installs["kernel-only"] / "omegon")
        shutil.copy2(kernel_binary, installs["kernel+codescan"] / "omegon")
        shutil.copy2(full_binary, installs["full-product"] / "omegon")
        sidecar_roots = {}
        for name in ("kernel+codescan", "full-product"):
            sidecar_root = installs[name] / "share/omegon/extensions/omegon-codescan"
            (sidecar_root / "target/release").mkdir(parents=True)
            shutil.copy2(
                ROOT / "extensions/omegon-codescan/manifest.toml",
                sidecar_root / "manifest.toml",
            )
            shutil.copy2(
                ROOT / "extensions/omegon-codescan/target/release/omegon-codescan",
                sidecar_root / "target/release/omegon-codescan",
            )
            component_lock = package_release.codescan_component_lock(
                (sidecar_root / "manifest.toml").read_bytes(),
                (sidecar_root / "target/release/omegon-codescan").read_bytes(),
                target_name,
                "https://github.com/styrene-lab/omegon/.github/workflows/release.yml@refs/tags/v0.0.0",
                "required",
            )
            lock_path = installs[name] / package_release.CODESCAN_COMPONENT_LOCK
            lock_path.parent.mkdir(parents=True)
            lock_path.write_bytes(package_release.canonical_json(component_lock))
            sidecar_roots[name] = sidecar_root
        if digest(installs["kernel-only"] / "omegon") != digest(
            installs["kernel+codescan"] / "omegon"
        ):
            raise ValueError("additive codescan composition changed the kernel host bytes")

        payloads = {}
        for name, probe in (("kernel-only", "core-read"), ("kernel+codescan", "codescan-search")):
            workspace = root / f"workspace-{name}"
            home = root / f"home-{name}"
            workspace.mkdir()
            home.mkdir()
            (workspace / "composition-probe.txt").write_text(
                "omegon-composition-core-probe\n"
            )
            (workspace / "codescan_probe.rs").write_text(
                "pub fn omegon_composition_codescan_probe() -> bool { true }\n"
            )
            env = os.environ.copy()
            env.update({"HOME": str(home), "OMEGON_HOME": str(home), "OMEGON_LOG": "error"})
            payload = run_json(
                [
                    str(installs[name] / "omegon"),
                    "--cwd",
                    str(workspace),
                    "composition-inspect",
                    "--profile",
                    "kernel",
                    "--probe",
                    probe,
                ],
                name,
                workspace,
                env,
            )
            if (
                payload.get("artifact_profile") != rows[name]["artifact_profile"]
                or payload.get("functional_probe", {}).get("status") != "ok"
            ):
                raise ValueError(f"{name}: functional artifact probe failed")
            payloads[name] = payload

        kernel = payloads["kernel-only"]
        additive = payloads["kernel+codescan"]
        if kernel["resident_capabilities"] != additive["resident_capabilities"]:
            raise ValueError("additive codescan composition changed resident host capabilities")
        if kernel["callable_capabilities"] != additive["callable_capabilities"]:
            raise ValueError("additive codescan composition changed the host callable surface")
        if kernel.get("external_processes") != []:
            raise ValueError("kernel-only unexpectedly admitted an external process")
        additive_processes = additive.get("external_processes")
        if (
            not isinstance(additive_processes, list)
            or len(additive_processes) != 1
            or additive_processes[0].get("owner") != "omegon-codescan"
            or additive_processes[0].get("state") != "healthy"
            or not isinstance(additive_processes[0].get("pid"), int)
        ):
            raise ValueError("kernel+codescan process delta is not exactly the declared sidecar")
        if process_is_running(additive_processes[0]["pid"]):
            raise ValueError("kernel+codescan sidecar survived deterministic host shutdown")
        if kernel["functional_probe"].get("codescan") != "service:unavailable":
            raise ValueError("kernel-only did not report typed codescan absence")
        provenance = additive["functional_probe"].get("service_provenance")
        if not isinstance(provenance, dict) or provenance.get("extension") != "omegon-codescan":
            raise ValueError("kernel+codescan did not report admitted process provenance")

        workspace = root / "workspace-full-product"
        home = root / "home-full-product"
        workspace.mkdir()
        home.mkdir()
        env = os.environ.copy()
        env.update({"HOME": str(home), "OMEGON_HOME": str(home), "OMEGON_LOG": "error"})
        product = run_json(
            [
                str(installs["full-product"] / "omegon"),
                "composition-inspect",
                "--profile",
                "full",
                "--cwd",
                str(workspace),
            ],
            "full-product",
            workspace,
            env,
            180,
        )
        validate_profile(product, "full", policy["profiles"]["full"])
        payloads["full-product"] = product

        evidence = {
            "schema_version": 1,
            "target": target_name,
            "cargo_profile": cargo_profile,
            "artifact_rows": {},
            "installed_full_product": installed_acceptance,
        }
        for name in REQUIRED_ARTIFACT_ROWS:
            sidecar = (
                sidecar_roots[name] / "target/release/omegon-codescan"
                if name in sidecar_roots
                else None
            )
            evidence["artifact_rows"][name] = {
                "install_root": str(installs[name]),
                "host_binary": str(installs[name] / "omegon"),
                "sidecar_binary": None if sidecar is None else str(sidecar),
                "inspection": payloads[name],
            }
        evidence_path = root / "artifact-evidence.json"
        evidence_path.write_text(json.dumps(evidence, indent=2, sort_keys=True) + "\n")
        budget_command = [
            sys.executable,
            "scripts/check_composition_budgets.py",
            "--artifact-evidence",
            str(evidence_path),
        ]
        if budget_output is not None:
            budget_command.extend(["--output", str(budget_output.resolve())])
        subprocess.run(budget_command, cwd=ROOT, check=True)


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
        if path == "release":
            exercise_installed_full_product(archive, target, policy["profiles"]["full"])


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--path", choices=sorted(REQUIRED_PATHS))
    parser.add_argument("--artifact-ladder", action="store_true")
    parser.add_argument("--binary-dir", type=Path)
    parser.add_argument("--archive", type=Path)
    parser.add_argument("--linked-home", type=Path)
    parser.add_argument("--target")
    parser.add_argument("--cargo-profile", default="release")
    parser.add_argument("--budget-output", type=Path)
    parser.add_argument("--policy", type=Path, default=POLICY)
    args = parser.parse_args()
    try:
        policy = load_policy(args.policy)
        if args.artifact_ladder:
            if args.path is not None:
                raise ValueError("artifact ladder and legacy path execution are mutually exclusive")
            exercise_source_artifact_ladder(policy, args.cargo_profile, args.budget_output)
            print("source artifact composition ladder passed")
            return 0
        if args.path is None:
            raise ValueError("provide --path or --artifact-ladder")
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
