import copy
import importlib.util
import json
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
POLICY_PATH = ROOT / "fixtures" / "release-composition-matrix-v1.json"
SCRIPT_PATH = ROOT / "scripts" / "check_distribution_policy.py"


def load_validator():
    spec = importlib.util.spec_from_file_location("check_distribution_policy", SCRIPT_PATH)
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(module)
    return module


class DistributionPolicyTests(unittest.TestCase):
    def setUp(self) -> None:
        self.validator = load_validator()
        self.policy = json.loads(POLICY_PATH.read_text())

    def assert_rejected(self, mutate) -> None:
        malformed = copy.deepcopy(self.policy)
        mutate(malformed)
        with self.assertRaises(ValueError):
            self.validator.validate_policy(malformed)

    def test_authoritative_policy_is_valid_and_matches_sources(self) -> None:
        self.validator.validate_policy(self.policy)
        self.validator.validate_source_evidence(self.policy, ROOT)

    def test_requires_exact_distribution_and_target_coverage(self) -> None:
        self.assert_rejected(lambda policy: policy["distribution_policy"]["rows"].pop())
        self.assert_rejected(
            lambda policy: policy["distribution_policy"]["rows"][0].update(
                target="invented-unknown-target"
            )
        )

    def test_rejects_missing_fields_unknown_ids_and_duplicate_components(self) -> None:
        self.assert_rejected(
            lambda policy: policy["distribution_policy"]["rows"][0].pop("host_profile")
        )
        self.assert_rejected(
            lambda policy: policy["distribution_policy"]["rows"][0].update(
                composition_class="unknown"
            )
        )
        self.assert_rejected(
            lambda policy: policy["distribution_policy"]["rows"][0].update(
                core_components=["core:codescan", "core:codescan"]
            )
        )
        self.assert_rejected(
            lambda policy: policy["distribution_policy"]["rows"][0].update(
                core_components=["core:unknown"]
            )
        )

    def test_sdk_extensions_cannot_self_promote_to_core(self) -> None:
        self.assert_rejected(
            lambda policy: policy["distribution_policy"]["rows"][0][
                "sdk_extensions"
            ].update(declared_core_components=["core:codescan"])
        )

    def test_host_only_requires_typed_absence_and_full_product_forbids_it(self) -> None:
        host_only = next(
            row
            for row in self.policy["distribution_policy"]["rows"]
            if row["composition_class"] == "host-only"
        )
        full_product = next(
            row
            for row in self.policy["distribution_policy"]["rows"]
            if row["composition_class"] == "full-product"
        )
        self.assert_rejected(
            lambda policy: next(
                row
                for row in policy["distribution_policy"]["rows"]
                if row["distribution"] == host_only["distribution"]
                and row["target"] == host_only["target"]
            ).pop("non_parity")
        )
        self.assert_rejected(
            lambda policy: next(
                row
                for row in policy["distribution_policy"]["rows"]
                if row["distribution"] == full_product["distribution"]
                and row["target"] == full_product["target"]
            ).update(non_parity=host_only["non_parity"])
        )

    def test_retained_npm_scaffolding_is_explicitly_unsupported(self) -> None:
        npm = self.policy["distribution_policy"]["unsupported"]["npm"]
        self.assertEqual(npm["status"], "unsupported")
        self.assertTrue(npm["retained_scaffolding"])

    def test_retained_npm_scaffolding_is_excluded_from_release_publication(self) -> None:
        release = (ROOT / ".github/workflows/release.yml").read_text()
        package_release = (ROOT / "scripts/package_release.py").read_text()
        release_manifest = (ROOT / "scripts/release_manifest.py").read_text()
        for source in (release, package_release, release_manifest):
            self.assertNotIn("core/npm", source)
            self.assertNotIn("npm publish", source)
            self.assertNotIn("npm pack", source)
        self.assertNotIn(".tgz", release)

    def test_distribution_runtime_smokes_are_wired_to_authoritative_lanes(self) -> None:
        test_workflow = (ROOT / ".github/workflows/test.yml").read_text()
        release_workflow = (ROOT / ".github/workflows/release.yml").read_text()
        homebrew_workflow = (ROOT / ".github/workflows/homebrew.yml").read_text()
        self.assertIn("scripts/smoke_distribution_runtime.py", test_workflow)
        self.assertIn("direct-installer", release_workflow)
        self.assertIn("nix-host", test_workflow)
        self.assertIn("oci-host", release_workflow)
        self.assertIn("smoke_distribution_runtime.py", homebrew_workflow)
        self.assertIn("homebrew", homebrew_workflow)
        self.assertNotIn("types: [published]", homebrew_workflow)
        self.assertIn("publish_package_channels", release_workflow)
        self.assertGreaterEqual(
            release_workflow.count("inputs.publish_package_channels"), 2
        )

    def test_host_only_package_metadata_and_installer_test_seam_are_declared(self) -> None:
        installer = (ROOT / "core/install.sh").read_text()
        flake = (ROOT / "flake.nix").read_text()
        oci = (ROOT / "nix/oci.nix").read_text()
        self.assertIn("OMEGON_INSTALL_ARCHIVE", installer)
        self.assertIn("OMEGON_INSTALL_CHECKSUMS", installer)
        self.assertIn("OMEGON_INSTALL_MANIFEST", installer)
        self.assertIn("OMEGON_INSTALL_BUNDLE", installer)
        self.assertIn("OMEGON_BOOTSTRAP_VERIFIER", installer)
        self.assertIn("distribution-profile.json", flake)
        self.assertIn('builtins.match ".*\\\\.py$"', flake)
        self.assertIn('builtins.match ".*\\\\.txt$"', flake)
        self.assertIn("pkgs.apple-sdk", flake)
        self.assertNotIn("darwin.apple_sdk.frameworks", flake)
        self.assertIn("sh.styrene.omegon.composition-class", oci)


if __name__ == "__main__":
    unittest.main()
