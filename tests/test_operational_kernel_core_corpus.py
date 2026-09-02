import copy
import unittest

from scripts import check_operational_kernel_core_corpus as corpus


class OperationalKernelCoreCorpusTests(unittest.TestCase):
    def setUp(self) -> None:
        self.document = corpus.load_corpus()

    def test_checked_in_corpus_is_valid(self) -> None:
        corpus.validate_corpus(self.document)

    def test_duplicate_scenario_id_is_rejected(self) -> None:
        forged = copy.deepcopy(self.document)
        forged["scenarios"].append(copy.deepcopy(forged["scenarios"][0]))

        with self.assertRaisesRegex(ValueError, "duplicate scenario id"):
            corpus.validate_corpus(forged)

    def test_unknown_axis_value_is_rejected(self) -> None:
        forged = copy.deepcopy(self.document)
        forged["scenarios"][0]["axes"]["artifact"] = "pretend-kernel"

        with self.assertRaisesRegex(ValueError, "unknown artifact value"):
            corpus.validate_corpus(forged)

    def test_implemented_evidence_requires_an_executor(self) -> None:
        forged = copy.deepcopy(self.document)
        forged["scenarios"][0]["evidence"]["executors"] = []

        with self.assertRaisesRegex(ValueError, "implemented evidence has no executor"):
            corpus.validate_corpus(forged)

    def test_scenario_evidence_cannot_bind_an_unrelated_executor(self) -> None:
        forged = copy.deepcopy(self.document)
        scenario = next(row for row in forged["scenarios"] if row["id"] == "LIF-001")
        scenario["evidence"] = {
            "status": "implemented",
            "executors": [
                {
                    "path": "core/crates/omegon/src/surface_parity_campaign.rs",
                    "command": "cargo test surface_parity_campaign::sur_001",
                }
            ],
        }

        with self.assertRaisesRegex(ValueError, "required evidence marker lif_001"):
            corpus.validate_corpus(forged)

    def test_scripted_kernel_baseline_is_complete(self) -> None:
        corpus.validate_corpus(self.document)
        self.assertEqual(
            corpus.incomplete_profile(self.document, "scripted-kernel-baseline"), []
        )

    def test_provider_backed_gate_is_complete(self) -> None:
        corpus.validate_corpus(self.document)
        self.assertEqual(
            corpus.incomplete_profile(self.document, "provider-backed-kernel"),
            [],
        )

    def test_composition_gates_are_complete(self) -> None:
        corpus.validate_corpus(self.document)
        for profile in ("signed-core-component", "sdk-addon"):
            with self.subTest(profile=profile):
                self.assertEqual(corpus.incomplete_profile(self.document, profile), [])

    def test_profile_commands_are_deduplicated_in_profile_order(self) -> None:
        commands = corpus.profile_commands(
            self.document,
            ["provider-backed-kernel", "signed-core-component", "sdk-addon"],
        )

        self.assertEqual(len(commands), len(set(commands)))
        self.assertEqual(
            commands[0],
            "cargo test -p omegon --locked --no-default-features --features kernel-host --test kernel_host_provider_blackbox",
        )

    def test_milestone_gate_contains_every_scenario(self) -> None:
        self.assertEqual(
            set(self.document["promotion_profiles"]["milestone-pr-readiness"]),
            {scenario["id"] for scenario in self.document["scenarios"]},
        )


if __name__ == "__main__":
    unittest.main()
