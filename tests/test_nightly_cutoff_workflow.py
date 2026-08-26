#!/usr/bin/env python3
"""Regression checks for the nightly cutoff and standard-release dispatch."""

from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]
WORKFLOW = (ROOT / ".github" / "workflows" / "nightly.yml").read_text()
CONTRIBUTING = (ROOT / "CONTRIBUTING.md").read_text()


class NightlyCutoffWorkflowTests(unittest.TestCase):
    def test_schedule_uses_immutable_0717_utc_cutoff(self) -> None:
        self.assertIn("cron: '17 7 * * *'", WORKFLOW)
        self.assertIn("fetch-depth: 0", WORKFLOW)
        self.assertNotIn("git push origin main", WORKFLOW)

    def test_new_tag_explicitly_dispatches_standard_release(self) -> None:
        self.assertIn("actions: write", WORKFLOW)
        self.assertIn("gh workflow run release.yml --ref \"$TAG\" -f tag_name=\"$TAG\"", WORKFLOW)
        self.assertIn("if: steps.exists.outputs.exists == 'false'", WORKFLOW)

    def test_existing_tag_is_never_replaced(self) -> None:
        self.assertIn("git ls-remote --tags origin", WORKFLOW)
        self.assertNotIn("git tag -f", WORKFLOW)
        self.assertNotIn("git push --force", WORKFLOW)

    def test_governance_documents_pr_gate_only_cutoff(self) -> None:
        self.assertIn("every day at **07:17 UTC**", CONTRIBUTING)
        self.assertIn("Required branch-protection checks and reviews are the complete pre-cut quality gate", CONTRIBUTING)
        self.assertIn("explicitly dispatches the standard release workflow", CONTRIBUTING)


if __name__ == "__main__":
    unittest.main()
