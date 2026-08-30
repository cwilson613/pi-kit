import hashlib
import importlib.util
import io
import json
import subprocess
import tarfile
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]


def load(name: str):
    path = ROOT / "scripts" / f"{name}.py"
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(module)
    return module


def release_archive(root: Path) -> Path:
    package_release = load("package_release")
    binaries = root / "bin"
    binaries.mkdir()
    for name in ("omegon", "omegon-maintain"):
        path = binaries / name
        path.write_bytes(name.encode())
        path.chmod(0o755)
    codescan = root / "omegon-codescan"
    codescan.write_bytes(b"codescan")
    codescan.chmod(0o755)
    archive = root / "omegon-0.0.0-x86_64-unknown-linux-gnu.tar.gz"
    package_release.package(
        binaries,
        archive,
        codescan,
        ROOT / "extensions/omegon-codescan/manifest.toml",
    )
    return archive


def mutate_archive(archive: Path, mutation: str) -> None:
    with tarfile.open(archive, "r:gz") as package:
        members = [
            (member.name, package.extractfile(member).read(), member.mode)
            for member in package
            if member.isfile()
        ]
    manifest = "share/omegon/extensions/omegon-codescan/manifest.toml"
    executable = "share/omegon/extensions/omegon-codescan/target/release/omegon-codescan"
    if mutation == "missing":
        members = [member for member in members if member[0] != manifest]
    elif mutation == "duplicate":
        members.append(next(member for member in members if member[0] == executable))
    elif mutation == "misplaced":
        members = [
            ("share/omegon/extensions/omegon-codescan/omegon-codescan", payload, mode)
            if name == executable
            else (name, payload, mode)
            for name, payload, mode in members
        ]
    elif mutation == "unexpected":
        members.append(("share/omegon/extensions/omegon-codescan/README.md", b"extra", 0o644))
    else:
        raise AssertionError(f"unknown archive mutation: {mutation}")
    with tarfile.open(archive, "w:gz") as package:
        for name, payload, mode in members:
            member = tarfile.TarInfo(name)
            member.mode = mode
            member.size = len(payload)
            package.addfile(member, io.BytesIO(payload))


def full_product_inspection(*, denied: bool, unavailable: bool = False) -> dict:
    callable_capabilities = ["tool:read"]
    probe = {
        "name": "codescan-search",
        "status": "disabled" if denied else "ok",
    }
    processes = []
    if unavailable:
        probe.update(
            {
                "status": "unavailable",
                "code": "service:unavailable",
                "component_id": "core:codescan",
            }
        )
    elif denied:
        probe.update(
            {
                "code": "service:disabled",
                "component_id": "core:codescan",
            }
        )
    else:
        callable_capabilities.extend(["tool:codebase_index", "tool:codebase_search"])
        probe["service_provenance"] = {
            "extension": "omegon-codescan",
            "source_digest": "fixture-source-digest",
            "pid": 999_999_999,
        }
        probe.update(
            {
                "component_id": "core:codescan",
                "wire_manifest_id": "omegon-codescan",
                "service_id": "service:codescan",
                "protocol_version": 1,
            }
        )
        processes.append(
            {
                "owner": "omegon-codescan",
                "state": "healthy",
                "pid": 999_999_999,
            }
        )
    return {
        "schema_version": 1,
        "profile": "full",
        "artifact_profile": "full-product",
        "canonical_entrypoint": ["omegon"],
        "runtime_mode": "full",
        "surfaces": ["agent-loop", "bounded-task", "control-plane", "tui"],
        "absent_optional": [],
        "startup_tasks": {"count": 1, "owners": {"system:test": 1}},
        "model_schema": {"count": 1, "owners": {"system:test": 1}},
        "resident_capabilities": [
            "system:constitutional-kernel",
            "system:default-loop",
            "system:host-effects",
            "feature:codescan-adapter",
            "feature:context-compaction",
            "feature:git",
            "feature:lifecycle",
            "feature:memory",
        ],
        "callable_capabilities": callable_capabilities,
        "external_processes": processes,
        "functional_probe": probe,
    }


class CompositionReleaseGateTests(unittest.TestCase):
    def test_matrix_declares_positive_additive_artifact_ladder(self) -> None:
        matrix = load("check_composition_matrix")
        policy = matrix.load_policy()
        rows = policy["artifact_rows"]
        self.assertEqual(set(rows), {"kernel-only", "kernel+codescan", "full-product"})

        kernel = rows["kernel-only"]
        self.assertEqual(kernel["artifact_profile"], "kernel-host-v1")
        self.assertEqual(kernel["cargo"]["bin"], "omegon-kernel-host")
        self.assertEqual(kernel["cargo"]["default_features"], False)
        self.assertTrue(kernel["positive_boundary"]["dependency_roots"])
        self.assertEqual(
            kernel["positive_boundary"]["resident_capabilities"],
            [
                "system:constitutional-kernel",
                "system:default-loop",
                "system:host-effects",
                "feature:codescan-adapter",
            ],
        )

        additive = rows["kernel+codescan"]
        self.assertEqual(additive["host_artifact"], "kernel-only")
        self.assertEqual(additive["extensions"], ["omegon-codescan"])
        self.assertEqual(additive["restores"], ["service:codescan"])
        self.assertNotEqual(
            additive["installation_identity"], kernel["installation_identity"]
        )
        self.assertEqual(rows["full-product"]["artifact_profile"], "full-product")

    def test_matrix_rejects_runtime_aliases_as_artifact_rows(self) -> None:
        matrix = load("check_composition_matrix")
        policy = matrix.load_policy()
        forged = json.loads(json.dumps(policy))
        forged["artifact_rows"]["full-product"]["installation_identity"] = forged[
            "artifact_rows"
        ]["kernel-only"]["installation_identity"]
        with self.assertRaisesRegex(ValueError, "installation identity"):
            matrix.validate_artifact_rows(forged["artifact_rows"])

        forged = json.loads(json.dumps(policy))
        forged["artifact_rows"]["kernel+codescan"]["host_artifact"] = "full-product"
        with self.assertRaisesRegex(ValueError, "kernel-only host"):
            matrix.validate_artifact_rows(forged["artifact_rows"])

    def test_extracted_domain_scenario_requires_each_declared_row(self) -> None:
        matrix = load("check_composition_matrix")
        policy = matrix.load_policy()
        for role in (
            "kernel_absence",
            "additive_restoration",
            "accumulated_product",
        ):
            with self.subTest(role=role):
                forged = json.loads(json.dumps(policy))
                forged["extracted_domains"]["codescan"][role]["row"] = "missing-row"
                with self.assertRaisesRegex(ValueError, "missing artifact row"):
                    matrix.validate_extracted_domains(
                        forged["extracted_domains"], forged["artifact_rows"]
                    )

    def test_extracted_domain_scenario_rejects_aliased_rows(self) -> None:
        matrix = load("check_composition_matrix")
        policy = matrix.load_policy()
        declaration = policy["extracted_domains"]["codescan"]
        declaration["accumulated_product"]["row"] = declaration[
            "additive_restoration"
        ]["row"]
        with self.assertRaisesRegex(ValueError, "must not alias"):
            matrix.validate_extracted_domains(
                policy["extracted_domains"], policy["artifact_rows"]
            )

    def test_extracted_domain_scenario_rejects_mismatched_identities(self) -> None:
        matrix = load("check_composition_matrix")
        policy = matrix.load_policy()
        mutations = {
            "service": ("additive_restoration", "restores", "service:other"),
            "extension": ("accumulated_product", "extensions", "omegon-other"),
        }
        for identity, (role, field, value) in mutations.items():
            with self.subTest(identity=identity):
                forged = json.loads(json.dumps(policy))
                forged["extracted_domains"]["codescan"][role]["evidence"][field] = value
                with self.assertRaisesRegex(ValueError, "identity is mismatched"):
                    matrix.validate_extracted_domains(
                        forged["extracted_domains"], forged["artifact_rows"]
                    )

    def test_extracted_domain_scenario_rejects_absence_after_restoration(self) -> None:
        matrix = load("check_composition_matrix")
        policy = matrix.load_policy()
        policy["artifact_rows"]["kernel+codescan"]["typed_unavailable"].append(
            "service:codescan"
        )
        with self.assertRaisesRegex(ValueError, "absence remains"):
            matrix.validate_extracted_domains(
                policy["extracted_domains"], policy["artifact_rows"]
            )

    def test_extracted_domain_scenario_requires_full_product_retention(self) -> None:
        matrix = load("check_composition_matrix")
        policy = matrix.load_policy()
        policy["artifact_rows"]["full-product"]["extensions"].remove(
            "omegon-codescan"
        )
        with self.assertRaisesRegex(ValueError, "evidence is absent"):
            matrix.validate_extracted_domains(
                policy["extracted_domains"], policy["artifact_rows"]
            )

    def test_kernel_dependency_policy_inventory_rejects_every_product_root(self) -> None:
        boundary = load("check_kernel_dependency_boundary")
        output = "\n".join(
            f"{package} v1.0.0" for package in boundary.FORBIDDEN_PRODUCT_PACKAGES
        )
        self.assertEqual(
            boundary.product_packages(output),
            sorted(boundary.FORBIDDEN_PRODUCT_PACKAGES),
        )
        self.assertEqual(boundary.product_packages("omegon-traits v1.0.0"), [])

    def test_kernel_manifest_selects_distinct_targets_and_product_owns_domains(self) -> None:
        boundary = load("check_kernel_dependency_boundary")
        manifest = boundary.tomllib.loads(boundary.MANIFEST.read_text())
        self.assertEqual(boundary.manifest_errors(manifest), [])

    def test_kernel_dependency_policy_rejects_undeclared_enabled_direct_root(self) -> None:
        boundary = load("check_kernel_dependency_boundary")
        declared = {"anyhow", "serde"}
        self.assertEqual(
            boundary.direct_root_errors(declared, declared | {"reqwest"}),
            ["enabled direct roots are undeclared: reqwest"],
        )

    def test_kernel_dependency_policy_rejects_declared_but_disabled_root(self) -> None:
        boundary = load("check_kernel_dependency_boundary")
        self.assertEqual(
            boundary.direct_root_errors({"anyhow", "serde"}, {"anyhow"}),
            ["declared direct roots are disabled: serde"],
        )

    def test_matrix_names_every_profile_path_and_runtime_state(self) -> None:
        matrix = load("check_composition_matrix")
        policy = matrix.load_policy()
        self.assertEqual(set(policy["profiles"]), matrix.REQUIRED_PROFILES)
        self.assertTrue(policy["profiles"]["maintenance"]["absent_optional"])
        self.assertEqual(policy["profiles"]["headless"]["runtime_mode"], "headless")
        self.assertEqual(policy["profiles"]["daemon"]["surfaces"], ["agent-loop", "control-plane"])

    def test_codescan_host_identity_is_the_adapter(self) -> None:
        matrix = load("check_composition_matrix")
        policy = matrix.load_policy()
        for profile in ("interactive", "headless", "daemon", "full"):
            identities = policy["profiles"][profile]["resident_capabilities"]
            self.assertIn("feature:codescan-adapter", identities, profile)
            self.assertNotIn("feature:codescan", identities, profile)

    def test_generated_resident_lock_attributes_only_the_adapter_to_omegon(self) -> None:
        package_release = load("package_release")
        lock = json.loads(
            package_release.resident_lock(
                "omegon",
                b"omegon",
                "x86_64-unknown-linux-gnu",
                "test:workflow",
                "required",
            )
        )
        identities = {entry["identity"] for entry in lock["contributions"]}
        self.assertIn("feature:codescan-adapter", identities)
        self.assertNotIn("feature:codescan", identities)

    def test_release_archive_accepts_the_declared_codescan_sidecar(self) -> None:
        matrix = load("check_composition_matrix")
        with tempfile.TemporaryDirectory() as directory:
            archive = release_archive(Path(directory))
            matrix.verify_archive_inventory(archive, "x86_64-unknown-linux-gnu")

    def test_release_archive_rejects_mutated_component_lock_and_payloads(self) -> None:
        matrix = load("check_composition_matrix")
        package_release = load("package_release")
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            archive = release_archive(root)
            extracted = matrix.verify_archive_inventory(archive, "x86_64-unknown-linux-gnu")
            lock = json.loads((extracted / package_release.CODESCAN_COMPONENT_LOCK).read_text())
            for field, value in (
                ("component_id", "executable:omegon"),
                ("wire_manifest_id", "sdk:self-promoted"),
                ("target", "aarch64-apple-darwin"),
                ("protocol_version", 2),
                ("manifest_digest", "0" * 64),
                ("executable_digest", "0" * 64),
            ):
                forged = json.loads(json.dumps(lock))
                forged[field] = value
                with self.subTest(field=field), self.assertRaises(ValueError):
                    matrix.validate_product_component_lock(
                        forged, extracted, "x86_64-unknown-linux-gnu"
                    )

    def test_release_archive_rejects_inexact_codescan_inventory(self) -> None:
        matrix = load("check_composition_matrix")
        errors = {
            "missing": "lacks required extension members",
            "duplicate": "duplicate extension member",
            "misplaced": "misplaced extension member",
            "unexpected": "unexpected extension member",
        }
        for mutation, error in errors.items():
            with self.subTest(mutation=mutation), tempfile.TemporaryDirectory() as directory:
                archive = release_archive(Path(directory))
                mutate_archive(archive, mutation)
                with self.assertRaisesRegex(ValueError, error):
                    matrix.verify_archive_inventory(archive, "x86_64-unknown-linux-gnu")

    def test_installed_full_product_acceptance_preserves_package_and_rollback_evidence(self) -> None:
        matrix = load("check_composition_matrix")
        policy = matrix.load_policy()
        target = "x86_64-unknown-linux-gnu"
        with tempfile.TemporaryDirectory() as directory:
            archive = release_archive(Path(directory))

            def completed(command, profile, _cwd, env, _timeout=90):
                denied = Path(env["OMEGON_HOME"], "component-policy.json").exists()
                unavailable = "unavailable-home" in env["OMEGON_HOME"]
                payload = full_product_inspection(denied=denied, unavailable=unavailable)
                if not denied and not unavailable:
                    source = (
                        Path(command[0]).parent / matrix.package_release.CODESCAN_MANIFEST
                    ).parent
                    payload["functional_probe"]["service_provenance"]["source_digest"] = (
                        matrix.dynamic_source_digest(source)
                    )
                if denied:
                    payload["functional_probe"]["determining_policy_source"] = {
                        "kind": "user-local",
                        "path": str(Path(env["OMEGON_HOME"], "component-policy.json")),
                    }
                return payload

            with patch.object(matrix, "run_json", side_effect=completed):
                evidence = matrix.exercise_installed_full_product(
                    archive, target, policy["profiles"]["full"]
                )

        self.assertEqual(evidence["default"]["inventory"], evidence["denied"]["inventory"])
        self.assertEqual(evidence["default"]["member_digests"], evidence["denied"]["member_digests"])
        self.assertEqual(evidence["default"]["resident_locks"], evidence["denied"]["resident_locks"])
        self.assertEqual(evidence["default"]["package_manifest"], evidence["denied"]["package_manifest"])
        self.assertTrue(evidence["rollback_valid"])
        self.assertTrue(evidence["missing_codescan_rejected_under_deny"])

    def test_installed_full_product_acceptance_proves_runtime_policy_boundary(self) -> None:
        matrix = load("check_composition_matrix")
        policy = matrix.load_policy()
        target = "x86_64-unknown-linux-gnu"
        with tempfile.TemporaryDirectory() as directory:
            archive = release_archive(Path(directory))

            def completed(command, profile, _cwd, env, _timeout=90):
                denied = Path(env["OMEGON_HOME"], "component-policy.json").exists()
                unavailable = "unavailable-home" in env["OMEGON_HOME"]
                payload = full_product_inspection(denied=denied, unavailable=unavailable)
                if not denied and not unavailable:
                    source = (
                        Path(command[0]).parent / matrix.package_release.CODESCAN_MANIFEST
                    ).parent
                    payload["functional_probe"]["service_provenance"]["source_digest"] = (
                        matrix.dynamic_source_digest(source)
                    )
                if denied:
                    payload["functional_probe"]["determining_policy_source"] = {
                        "kind": "user-local",
                        "path": str(Path(env["OMEGON_HOME"], "component-policy.json")),
                    }
                return payload

            with patch.object(matrix, "run_json", side_effect=completed):
                evidence = matrix.exercise_installed_full_product(
                    archive, target, policy["profiles"]["full"]
                )

        self.assertEqual(evidence["default"]["probe"]["status"], "ok")
        self.assertEqual(evidence["denied"]["probe"]["code"], "service:disabled")
        self.assertNotIn("tool:codebase_index", evidence["denied"]["callable_capabilities"])
        self.assertNotIn("tool:codebase_search", evidence["denied"]["callable_capabilities"])
        self.assertEqual(evidence["denied"]["external_processes"], [])
        self.assertEqual(
            evidence["denied"]["probe"]["determining_policy_source"]["kind"],
            "user-local",
        )

    def test_source_path_executes_every_profile_through_cargo(self) -> None:
        matrix = load("check_composition_matrix")
        policy = matrix.load_policy()
        commands = []

        def completed(command, **_kwargs):
            commands.append(command)
            if "omegon-maintain" in command:
                payload = {"composition": {"profile": "maintenance"}}
            else:
                profile = command[command.index("composition-inspect") + 2]
                row = policy["profiles"][profile]
                payload = {
                    "schema_version": 1,
                    "profile": profile,
                    "artifact_profile": row["artifact_profile"],
                    "canonical_entrypoint": row["canonical_entrypoint"],
                    "runtime_mode": row["runtime_mode"],
                    "surfaces": row["surfaces"],
                    "absent_optional": row["absent_optional"],
                    "startup_tasks": {"count": 1, "owners": {"system:test": 1}},
                    "model_schema": {"count": 2, "owners": {"feature:test": 2}},
                    "resident_capabilities": row["resident_capabilities"],
                    "callable_capabilities": ["tool:test"],
                }
            return subprocess.CompletedProcess(command, 0, json.dumps(payload), "")

        with patch.object(matrix.subprocess, "run", side_effect=completed):
            matrix.exercise(
                "source",
                policy,
                binary_dir=None,
                archive=None,
                linked_home=None,
                target=None,
                cargo_profile="release",
            )
        self.assertEqual(len(commands), len(matrix.REQUIRED_PROFILES))
        self.assertTrue(all(command[:2] == ["cargo", "run"] for command in commands))

    def test_profile_validation_rejects_forged_artifact_identity(self) -> None:
        matrix = load("check_composition_matrix")
        row = matrix.load_policy()["profiles"]["headless"]
        payload = {
            "schema_version": 1,
            "profile": "headless",
            "artifact_profile": "task-capsule-v0",
            "canonical_entrypoint": ["omegon", "run"],
            "runtime_mode": row["runtime_mode"],
            "surfaces": row["surfaces"],
            "absent_optional": row["absent_optional"],
        }
        with self.assertRaisesRegex(ValueError, "artifact profile"):
            matrix.validate_profile(payload, "headless", row)

    def test_linked_path_fails_when_installed_launchers_are_absent(self) -> None:
        matrix = load("check_composition_matrix")
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            binaries = root / "bin"
            binaries.mkdir()
            for name in matrix.EXECUTABLES:
                path = binaries / name
                path.write_bytes(name.encode())
                path.chmod(0o755)
            with self.assertRaisesRegex(ValueError, "linked launcher"):
                matrix.validate_linked_install(root / "home", binaries, "x86_64-unknown-linux-gnu")

    def test_resident_lock_rejects_unknown_duplicate_and_mismatched_entries(self) -> None:
        matrix = load("check_composition_matrix")
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            executable = root / "omegon-maintain"
            executable.write_bytes(b"maintain")
            executable.chmod(0o755)
            base = {
                "schema_version": 1,
                "executable_identity": "omegon-maintain",
                "executable_digest": hashlib.sha256(b"maintain").hexdigest(),
                "target": "x86_64-unknown-linux-gnu",
                "protocol_minimum": 1,
                "protocol_maximum": 1,
                "signing_identity": {
                    "issuer": "https://token.actions.githubusercontent.com",
                    "verification": "required",
                    "workflow_identity": "test:workflow",
                },
                "contributions": [
                    {
                        "identity": "system:maintenance-kernel",
                        "artifact_path": "omegon-maintain",
                        "artifact_digest": hashlib.sha256(b"maintain").hexdigest(),
                        "targets": ["x86_64-unknown-linux-gnu"],
                        "protocol_minimum": 1,
                        "protocol_maximum": 1,
                        "required": True,
                        "fallback": "fail_closed",
                        "state": "resident",
                    }
                ],
            }
            lock = root / "omegon-maintain.composition-lock.json"
            lock.write_text(json.dumps(base))
            matrix.validate_resident_lock(
                lock,
                executable,
                "x86_64-unknown-linux-gnu",
                "required",
                "test:workflow",
            )
            for mutation in ("identity", "artifact_path", "targets", "state"):
                malformed = json.loads(json.dumps(base))
                malformed["contributions"][0][mutation] = "wrong" if mutation != "targets" else ["wrong"]
                lock.write_text(json.dumps(malformed))
                with self.assertRaises(ValueError, msg=mutation):
                    matrix.validate_resident_lock(
                        lock,
                        executable,
                        "x86_64-unknown-linux-gnu",
                        "required",
                        "test:workflow",
                    )
            malformed = json.loads(json.dumps(base))
            malformed["contributions"].append(malformed["contributions"][0])
            lock.write_text(json.dumps(malformed))
            with self.assertRaises(ValueError):
                matrix.validate_resident_lock(
                    lock,
                    executable,
                    "x86_64-unknown-linux-gnu",
                    "required",
                    "test:workflow",
                )

    def test_budget_gate_rejects_regressions_and_malformed_inputs(self) -> None:
        budgets = load("check_composition_budgets")
        policy = json.loads(budgets.POLICY.read_text())
        target = "aarch64-apple-darwin"
        measurement = {
            "schema_version": 1,
            "target": target,
            "profiles": {
                profile: {
                    "metrics": {
                        metric: budgets.metric_policy(policy, profile, metric, target)["baseline"]
                        for metric in budgets.METRICS
                    },
                    "owners": {},
                }
                for profile in ("maintenance", "normal")
            },
        }
        self.assertEqual(budgets.enforce(measurement, policy), [])
        measurement["profiles"]["maintenance"]["metrics"]["dependency_count"] = 10_000
        self.assertEqual(len(budgets.enforce(measurement, policy)), 1)
        malformed = json.loads(json.dumps(measurement))
        del malformed["target"]
        with self.assertRaises(ValueError):
            budgets.enforce(malformed, policy)
        malformed_policy = json.loads(json.dumps(policy))
        malformed_policy["profiles"]["normal"]["startup_task_count"]["max_delta"] = "many"
        with self.assertRaises(ValueError):
            budgets.enforce(measurement, malformed_policy)

    def test_authority_source_guard_passes(self) -> None:
        guard = load("check_composition_authority")
        self.assertEqual(guard.main(), 0)


if __name__ == "__main__":
    unittest.main()
