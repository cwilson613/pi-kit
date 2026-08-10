#!/usr/bin/env python3
"""Regression checks for nightly releases using the standard CI release path."""

from pathlib import Path
import unittest

import yaml

ROOT = Path(__file__).resolve().parents[1]
WORKFLOW = ROOT / ".github" / "workflows" / "release.yml"
JUSTFILE = (ROOT / "Justfile").read_text()


class NightlyStandardReleaseTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.text = WORKFLOW.read_text()
        cls.workflow = yaml.safe_load(cls.text)

    def test_standard_release_workflow_accepts_nightly_tags(self) -> None:
        triggers = self.workflow[True]
        self.assertEqual(triggers["push"]["tags"], ["v*"])
        self.assertIn("workflow_dispatch", triggers)
        self.assertIn("contains(env.RELEASE_TAG, '-nightly.')", self.text)

    def test_nightly_uses_standard_ci_signing_and_notarization_credentials(self) -> None:
        env = self.workflow["env"]
        for name in (
            "APPLE_CODESIGN_IDENTITY",
            "APPLE_SIGNING_CERT_B64",
            "APPLE_SIGNING_CERT_PASSWORD",
            "APPLE_API_KEY_P8_B64",
            "APPLE_API_KEY_ID",
            "APPLE_API_ISSUER",
        ):
            self.assertIn(name, env)
            self.assertIn("secrets.", env[name])

        self.assertIn("Import Apple signing certificate", self.text)
        self.assertIn("codesign --force --options runtime --timestamp", self.text)
        self.assertIn("xcrun notarytool submit", self.text)

    def test_manual_handoff_path_is_absent(self) -> None:
        self.assertFalse(
            (ROOT / ".github" / "workflows" / "publish-notarized-nightly.yml").exists()
        )
        self.assertNotIn("prepare-nightly-notarization", JUSTFILE)
        self.assertNotIn("finalize-nightly tag=", JUSTFILE)


if __name__ == "__main__":
    unittest.main()
