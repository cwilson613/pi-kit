import hashlib
import importlib.util
import json
import subprocess
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


class CompositionReleaseGateTests(unittest.TestCase):
    def test_matrix_names_every_profile_path_and_runtime_state(self) -> None:
        matrix = load("check_composition_matrix")
        policy = matrix.load_policy()
        self.assertEqual(set(policy["profiles"]), matrix.REQUIRED_PROFILES)
        self.assertTrue(policy["profiles"]["maintenance"]["absent_optional"])
        self.assertEqual(policy["profiles"]["headless"]["runtime_mode"], "headless")
        self.assertEqual(policy["profiles"]["daemon"]["surfaces"], ["agent-loop", "control-plane"])

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
