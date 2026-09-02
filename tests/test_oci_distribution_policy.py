import copy
import importlib.util
import json
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
POLICY_PATH = ROOT / "fixtures" / "release-composition-matrix-v1.json"
FIXTURE_PATH = ROOT / "fixtures" / "oci-release-evidence-valid-v1.json"
SCRIPT_PATH = ROOT / "scripts" / "check_distribution_policy.py"


def load_validator():
    spec = importlib.util.spec_from_file_location("check_distribution_policy", SCRIPT_PATH)
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(module)
    return module


class OciDistributionPolicyTests(unittest.TestCase):
    def setUp(self) -> None:
        self.validator = load_validator()
        self.policy = json.loads(POLICY_PATH.read_text())
        self.evidence = json.loads(FIXTURE_PATH.read_text())

    def assert_rejected(self, mutate) -> None:
        malformed = copy.deepcopy(self.evidence)
        mutate(malformed)
        with self.assertRaises(ValueError):
            self.validator.validate_oci_release_evidence(malformed, self.policy)

    def test_complete_digest_bound_policy_fixture_is_accepted(self) -> None:
        self.validator.validate_oci_release_evidence(self.evidence, self.policy)

        full_product = copy.deepcopy(self.evidence)
        full_product["composition_class"] = "full-product"
        full_product["core_components"] = ["core:codescan"]
        for item in full_product["evidence"].values():
            item["composition_class"] = "full-product"
        full_product["evidence"]["composition_identity"]["core_components"] = [
            "core:codescan"
        ]
        self.validator.validate_oci_release_evidence(full_product, self.policy)

    def test_incomplete_and_cross_digest_fixtures_are_rejected(self) -> None:
        for name in (
            "oci-release-evidence-incomplete-v1.json",
            "oci-release-evidence-digest-mismatch-v1.json",
        ):
            with self.subTest(name=name), self.assertRaises(ValueError):
                fixture = json.loads((ROOT / "fixtures" / name).read_text())
                self.validator.validate_oci_release_evidence(fixture, self.policy)

    def test_each_required_evidence_document_is_mandatory(self) -> None:
        for name in ("signature", "sbom", "provenance", "composition_identity"):
            with self.subTest(name=name):
                self.assert_rejected(lambda evidence, name=name: evidence["evidence"].pop(name))

    def test_rejects_malformed_digest_and_mutable_tag_substitution(self) -> None:
        self.assert_rejected(lambda evidence: evidence.update(image_digest="sha256:not-a-digest"))
        self.assert_rejected(
            lambda evidence: evidence.update(image_reference="omegon-policy-fixture:latest")
        )

    def test_rejects_cross_digest_and_cross_class_evidence(self) -> None:
        self.assert_rejected(
            lambda evidence: evidence["evidence"]["sbom"].update(
                image_digest="sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
            )
        )
        self.assert_rejected(
            lambda evidence: evidence["evidence"]["signature"].update(
                composition_class="full-product"
            )
        )

    def test_rejects_unknown_class_and_false_full_product_inventory(self) -> None:
        self.assert_rejected(lambda evidence: evidence.update(composition_class="unknown"))

        def claim_full_product(evidence) -> None:
            evidence["composition_class"] = "full-product"
            for item in evidence["evidence"].values():
                item["composition_class"] = "full-product"

        self.assert_rejected(claim_full_product)

    def test_rejects_publication_or_live_verification_claims(self) -> None:
        self.assert_rejected(lambda evidence: evidence.update(publication_status="published"))
        self.assert_rejected(lambda evidence: evidence.update(verification_scope="live-registry"))


if __name__ == "__main__":
    unittest.main()
