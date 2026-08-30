import importlib.util
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
REQUIRED_MODULES = {
    "tests.test_composition_release_gates",
    "tests.test_content_pack_packaging",
    "tests.test_distribution_policy",
    "tests.test_distribution_runtime_smoke",
    "tests.test_optional_domain_isolation",
    "tests.test_release_closeout",
    "tests.test_release_manifest",
    "tests.test_release_preflight",
    "tests.test_release_status",
    "tests.test_validate_companion",
    "tests.test_verify_homebrew_formula",
}


class ReleasePolicyWorkflowTests(unittest.TestCase):
    def test_release_policy_runner_covers_maintained_modules(self) -> None:
        path = ROOT / "scripts/test_release_policy.py"
        spec = importlib.util.spec_from_file_location("test_release_policy", path)
        module = importlib.util.module_from_spec(spec)
        assert spec.loader is not None
        spec.loader.exec_module(module)
        self.assertTrue(REQUIRED_MODULES.issubset(set(module.TEST_MODULES)))

    def test_pull_request_workflow_runs_the_release_policy_runner(self) -> None:
        workflow = (ROOT / ".github/workflows/test.yml").read_text()
        self.assertIn("python-release-policy:", workflow)
        self.assertIn("python3 scripts/test_release_policy.py", workflow)

    def test_rust_test_recipes_select_the_canonical_glyph_set(self) -> None:
        recipes = (ROOT / "Justfile").read_text().splitlines()
        cargo_test_lines = [line for line in recipes if "{{cargo}} test" in line]
        self.assertTrue(cargo_test_lines)
        self.assertEqual(
            [],
            [line for line in cargo_test_lines if "OMEGON_NERD_FONT=1" not in line],
        )


if __name__ == "__main__":
    unittest.main()
