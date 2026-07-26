#!/usr/bin/env python3
"""Regression tests for stable-release/trunk version invariants."""

from __future__ import annotations

import importlib.util
import subprocess
import tempfile
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).with_name("release_branch.py")
SPEC = importlib.util.spec_from_file_location("release_branch", MODULE_PATH)
assert SPEC and SPEC.loader
release_branch = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(release_branch)


class PublishInvariantTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory()
        self.repo = Path(self.temp.name)
        subprocess.run(["git", "init", "-q", "--initial-branch=main"], cwd=self.repo, check=True)
        subprocess.run(["git", "config", "user.email", "test@example.com"], cwd=self.repo, check=True)
        subprocess.run(["git", "config", "user.name", "Test"], cwd=self.repo, check=True)
        (self.repo / "Cargo.toml").write_text('[workspace.package]\nversion = "0.28.7"\n')
        subprocess.run(["git", "add", "Cargo.toml"], cwd=self.repo, check=True)
        subprocess.run(["git", "commit", "-qm", "initial"], cwd=self.repo, check=True)
        subprocess.run(["git", "remote", "add", "origin", str(self.repo)], cwd=self.repo, check=True)
        subprocess.run(["git", "fetch", "-q", "origin", "main"], cwd=self.repo, check=True)

    def tearDown(self) -> None:
        self.temp.cleanup()

    def test_accepts_tag_on_trunk(self) -> None:
        # The normal case: tag cut from a commit that is on origin/main.
        release_branch.assert_tag_reachable_from_trunk(self.repo)

    def test_rejects_tag_not_reachable_from_trunk(self) -> None:
        # The failure this gate exists to catch: a tag cut from a commit that
        # never reached trunk. The old version-comparison check could not see
        # this at all.
        subprocess.run(["git", "checkout", "-q", "-b", "stray"], cwd=self.repo, check=True)
        (self.repo / "stray.txt").write_text("off-trunk")
        subprocess.run(["git", "add", "stray.txt"], cwd=self.repo, check=True)
        subprocess.run(["git", "commit", "-qm", "off trunk"], cwd=self.repo, check=True)
        with self.assertRaisesRegex(
            release_branch.ReleaseBranchError, "not reachable from origin/main"
        ):
            release_branch.assert_tag_reachable_from_trunk(self.repo)

    def test_verify_publish_accepts_detached_release_tag_checkout(self) -> None:
        subprocess.run(["git", "checkout", "--detach", "-q", "HEAD"], cwd=self.repo, check=True)

        release_branch.verify_publish_invariant(self.repo)

    def test_accepts_main_newer_than_release(self) -> None:
        release_branch.assert_main_version_not_behind(self.repo, "0.28.6")


class NextDevVersionTests(unittest.TestCase):
    def test_reopens_at_next_patch_dev(self) -> None:
        self.assertEqual(release_branch.next_dev_version("0.29.0"), "0.29.1-dev")
        self.assertEqual(release_branch.next_dev_version("0.28.11"), "0.28.12-dev")

    def test_rejects_prerelease_input(self) -> None:
        with self.assertRaises(release_branch.ReleaseBranchError):
            release_branch.next_dev_version("0.29.0-dev")

    def test_reopened_trunk_outranks_the_tag_just_published(self) -> None:
        # After tagging vX.Y.Z from trunk, trunk must sort strictly above it.
        for published in ("0.29.0", "0.28.11", "1.0.0"):
            reopened = release_branch.next_dev_version(published)
            self.assertGreater(
                release_branch.version_sort_key(reopened),
                release_branch.version_sort_key(published),
                f"trunk {reopened} must outrank published {published}",
            )


class NextTrunkVersionTests(unittest.TestCase):
    """Cutting release/X.Y must open X.(Y+1) on trunk.

    Regression for the 0.28.10 publish block: release/0.28 was cut while main
    stayed at 0.28.9, so tagging v0.28.10 tripped assert_main_version_not_behind.
    """

    def test_opens_next_minor_line(self) -> None:
        self.assertEqual(release_branch.next_trunk_version("0.28.9"), "0.29.0-dev")
        self.assertEqual(release_branch.next_trunk_version("1.4.0"), "1.5.0-dev")

    def test_next_trunk_version_outranks_any_patch_on_the_cut_line(self) -> None:
        # The whole point: no patch release on release/0.28 can ever be
        # "ahead of" trunk once trunk opens 0.29.
        trunk = release_branch.version_sort_key(release_branch.next_trunk_version("0.28.9"))
        for patch in ("0.28.10", "0.28.99"):
            self.assertGreater(trunk, release_branch.version_sort_key(patch))

    def test_rejects_prerelease_input(self) -> None:
        with self.assertRaises(release_branch.ReleaseBranchError):
            release_branch.next_trunk_version("0.29.0-dev")


if __name__ == "__main__":
    unittest.main()

