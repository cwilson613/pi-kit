#!/usr/bin/env python3
"""Regression checks for the local-to-CI notarized nightly handoff."""

from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]
JUSTFILE = (ROOT / "Justfile").read_text()


class NightlyNotarizationHandoffTests(unittest.TestCase):
    def test_local_recipe_never_publishes_release(self) -> None:
        recipe = JUSTFILE.split("prepare-nightly-notarization tag='':", 1)[1].split(
            "# Backward-compatible alias", 1
        )[0]
        self.assertNotIn("--draft=false", recipe)
        self.assertNotIn("gh release edit", recipe)
        self.assertIn("release remains draft", recipe)

    def test_local_recipe_builds_exact_tag_and_binds_remote_commit(self) -> None:
        self.assertIn('git worktree add --detach "$WORKTREE" "$TAG"', JUSTFILE)
        self.assertIn('REMOTE_TAG_COMMIT=$(gh api "repos/styrene-lab/omegon/commits/$TAG" --jq .sha)', JUSTFILE)
        self.assertIn('test "$TAG_COMMIT" = "$REMOTE_TAG_COMMIT"', JUSTFILE)
        self.assertIn(r'\"commit\":\"$TAG_COMMIT\"', JUSTFILE)

    def test_handoff_dispatches_reviewed_main_workflow(self) -> None:
        self.assertIn(
            'gh workflow run publish-notarized-nightly.yml --ref main -f tag_name="$TAG"',
            JUSTFILE,
        )

    def test_legacy_finalize_alias_delegates_without_reintroducing_publish(self) -> None:
        alias = JUSTFILE.split("finalize-nightly tag='':", 1)[1].split(
            "# ─── Armory", 1
        )[0]
        self.assertIn('just prepare-nightly-notarization "{{tag}}"', alias)
        self.assertNotIn("gh release", alias)


if __name__ == "__main__":
    unittest.main()
