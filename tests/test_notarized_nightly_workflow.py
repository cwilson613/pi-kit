#!/usr/bin/env python3
"""Regression checks for the local notarized-nightly publication handoff."""

from pathlib import Path
import unittest

import yaml

ROOT = Path(__file__).resolve().parents[1]
WORKFLOW = ROOT / ".github" / "workflows" / "publish-notarized-nightly.yml"


class NotarizedNightlyWorkflowTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.text = WORKFLOW.read_text()
        cls.workflow = yaml.safe_load(cls.text)

    def test_workflow_is_manual_and_uses_oidc(self) -> None:
        self.assertIn("workflow_dispatch", self.workflow[True])
        permissions = self.workflow["permissions"]
        self.assertEqual(permissions["contents"], "write")
        self.assertEqual(permissions["id-token"], "write")

    def test_handoff_is_bound_to_immutable_tag_commit(self) -> None:
        self.assertIn('test "$(jq -r .tag "$HANDOFF")" = "$TAG"', self.text)
        self.assertIn('test "$(jq -r .commit "$HANDOFF")" = "$TAG_COMMIT"', self.text)
        self.assertIn('test "$(jq -r .target "$HANDOFF")" = aarch64-apple-darwin', self.text)
        self.assertIn('test "$(jq -r .commit assets/release-manifest.json)" = "$TAG_COMMIT"', self.text)

    def test_archive_and_checksum_metadata_are_strictly_validated(self) -> None:
        self.assertIn('test "$ARCHIVE" = "$(basename "$ARCHIVE")"', self.text)
        self.assertIn('[[ "$EXPECTED" =~ ^[0-9a-f]{64}$ ]]', self.text)
        self.assertIn('expected exactly one manifest asset for ', self.text)
        self.assertNotIn("grep -v -- '-aarch64-apple-darwin.tar.gz'", self.text)
        self.assertNotIn("|| true", self.text)

    def test_post_handoff_metadata_is_resigned_and_uploaded(self) -> None:
        for asset in ("checksums.sha256", "release-manifest.json"):
            self.assertIn(f"cosign sign-blob --yes assets/{asset}", self.text)
            self.assertIn(f"assets/{asset}.sig", self.text)
            self.assertIn(f"assets/{asset}.pem", self.text)


if __name__ == "__main__":
    unittest.main()
