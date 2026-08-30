import importlib.util
import json
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def load_budgets():
    path = ROOT / "scripts/check_composition_budgets.py"
    spec = importlib.util.spec_from_file_location("check_composition_budgets", path)
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(module)
    return module


class AggregateCompositionBudgetScenarios(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.budgets = load_budgets()
        cls.policy = json.loads(cls.budgets.POLICY.read_text())
        cls.target = "aarch64-apple-darwin"

    def baseline_measurement(self) -> dict:
        rows = {}
        for name in self.budgets.ARTIFACT_ROWS:
            metrics = {
                metric: self.budgets.artifact_metric_policy(
                    self.policy, name, metric, self.target
                )["baseline"]
                for metric in self.budgets.ARTIFACT_METRICS
            }
            owners = {
                metric: (
                    {}
                    if value == 0
                    else {self.budgets.HOST_OWNER: value}
                )
                for metric, value in metrics.items()
            }
            rows[name] = {"metrics": metrics, "owners": owners}

        for name in ("kernel+codescan", "full-product"):
            sidecar_size = rows[name]["metrics"]["sidecar_binary_size_bytes"]
            rows[name]["owners"]["sidecar_binary_size_bytes"] = {
                self.budgets.SIDECAR_OWNER: sidecar_size
            }

        kernel = rows["kernel-only"]
        additive = rows["kernel+codescan"]
        for metric in (
            "sidecar_binary_size_bytes",
            "aggregate_installed_size_bytes",
            "dependency_count",
            "external_process_count",
        ):
            delta = additive["metrics"][metric] - kernel["metrics"][metric]
            additive["owners"][metric] = (
                {self.budgets.HOST_OWNER: kernel["metrics"][metric]}
                if kernel["metrics"][metric]
                else {}
            )
            if delta:
                additive["owners"][metric][self.budgets.SIDECAR_OWNER] = delta
        additive["owners"]["host_binary_size_bytes"] = {
            self.budgets.HOST_OWNER: additive["metrics"]["host_binary_size_bytes"]
        }
        return {
            "schema_version": 1,
            "target": self.target,
            "cargo_profile": "release",
            "artifact_rows": rows,
        }

    def test_each_required_cost_rejects_an_oversized_full_product(self) -> None:
        scenarios = (
            "host_binary_size_bytes",
            "sidecar_binary_size_bytes",
            "aggregate_installed_size_bytes",
            "dependency_count",
            "external_process_count",
            "startup_task_count",
            "model_schema_tokens",
            "resident_capability_count",
            "callable_capability_count",
        )
        for metric in scenarios:
            with self.subTest(metric=metric):
                measurement = self.baseline_measurement()
                approved = self.budgets.artifact_metric_policy(
                    self.policy, "full-product", metric, self.target
                )
                actual = approved["baseline"] + approved["max_delta"] + 1
                measurement["artifact_rows"]["full-product"]["metrics"][metric] = actual
                owner = (
                    self.budgets.SIDECAR_OWNER
                    if metric == "sidecar_binary_size_bytes"
                    else self.budgets.HOST_OWNER
                )
                measurement["artifact_rows"]["full-product"]["owners"][metric] = {
                    owner: actual
                }
                failures = self.budgets.enforce_artifacts(measurement, self.policy)
                self.assertEqual(len(failures), 1)
                self.assertIn(metric, failures[0])

    def test_missing_or_malformed_owner_diagnostics_are_rejected(self) -> None:
        measurement = self.baseline_measurement()
        del measurement["artifact_rows"]["full-product"]["owners"]["model_schema_tokens"]
        with self.assertRaisesRegex(ValueError, "owner diagnostics"):
            self.budgets.enforce_artifacts(measurement, self.policy)

        measurement = self.baseline_measurement()
        measurement["artifact_rows"]["full-product"]["owners"]["startup_task_count"] = {
            self.budgets.HOST_OWNER: -1
        }
        with self.assertRaisesRegex(ValueError, "malformed owner diagnostics"):
            self.budgets.enforce_artifacts(measurement, self.policy)

        measurement = self.baseline_measurement()
        measurement["artifact_rows"]["full-product"]["owners"]["dependency_count"] = {
            self.budgets.HOST_OWNER: 1
        }
        with self.assertRaisesRegex(ValueError, "malformed owner diagnostics"):
            self.budgets.enforce_artifacts(measurement, self.policy)

    def test_additive_row_rejects_changed_host_bytes(self) -> None:
        measurement = self.baseline_measurement()
        additive = measurement["artifact_rows"]["kernel+codescan"]
        additive["metrics"]["host_binary_size_bytes"] += 1
        additive["owners"]["host_binary_size_bytes"][self.budgets.HOST_OWNER] += 1
        with self.assertRaisesRegex(ValueError, "host bytes differ"):
            self.budgets.enforce_artifacts(measurement, self.policy)

    def test_policy_requires_every_supported_target_and_bounded_metric(self) -> None:
        self.budgets.validate_policy(self.policy)
        legacy = json.loads(json.dumps(self.policy))
        del legacy["artifact_rows"]
        self.budgets.validate_policy(legacy, artifact_required=False)

        malformed = json.loads(json.dumps(self.policy))
        del malformed["artifact_rows"]["kernel-only"]["host_binary_size_bytes"][
            "targets"
        ]["x86_64-unknown-linux-gnu"]
        with self.assertRaisesRegex(ValueError, "supported target budgets"):
            self.budgets.validate_policy(malformed)

        malformed = json.loads(json.dumps(self.policy))
        del malformed["artifact_rows"]["full-product"]["startup_task_count"][
            "max_delta"
        ]
        with self.assertRaisesRegex(ValueError, "baseline and max_delta"):
            self.budgets.validate_policy(malformed)


if __name__ == "__main__":
    unittest.main()
