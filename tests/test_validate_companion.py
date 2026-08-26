import os
from pathlib import Path
import tempfile
import unittest

from scripts.validate_companion import MAINTENANCE_EXCLUSIONS, validate


def executable(path: Path, body: str) -> None:
    path.write_text(f"#!/bin/sh\n{body}\n", encoding="utf-8")
    path.chmod(0o755)


def identity(version: str = "1.2.3", profile: str = "maintenance", exclusions=None) -> str:
    import json

    return json.dumps({
        "status": "success",
        "artifact": {
            "version": version,
            "commit": "abcdef0",
            "target": "test-target",
        },
        "composition": {
            "profile": profile,
            "excluded_inputs": MAINTENANCE_EXCLUSIONS if exclusions is None else exclusions,
        },
    })


class ValidateCompanionTests(unittest.TestCase):
    def test_accepts_matching_pair(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            executable(root / "omegon", "echo 'omegon 1.2.3 (abcdef0 2026-08-19)'")
            executable(
                root / "omegon-maintain",
                f"echo '{identity()}'",
            )
            validate(root / "omegon", root / "omegon-maintain", "test-target")

    def test_rejects_missing_companion(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            executable(root / "omegon", "echo 'omegon 1.2.3 (abcdef0 2026-08-19)'")
            with self.assertRaisesRegex(RuntimeError, "missing or not executable"):
                validate(root / "omegon", root / "omegon-maintain")

    def test_rejects_mismatched_companion(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            executable(root / "omegon", "echo 'omegon 1.2.3 (abcdef0 2026-08-19)'")
            executable(
                root / "omegon-maintain",
                f"echo '{identity(version='9.9.9')}'",
            )
            with self.assertRaisesRegex(RuntimeError, "version mismatch"):
                validate(root / "omegon", root / "omegon-maintain")

    def test_rejects_normal_runtime_profile(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            executable(root / "omegon", "echo 'omegon 1.2.3 (abcdef0 2026-08-19)'")
            executable(
                root / "omegon-maintain",
                f"echo '{identity(profile='full')}'",
            )
            with self.assertRaisesRegex(RuntimeError, "maintenance profile"):
                validate(root / "omegon", root / "omegon-maintain")

    def test_rejects_incomplete_exclusion_set(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            executable(root / "omegon", "echo 'omegon 1.2.3 (abcdef0 2026-08-19)'")
            executable(
                root / "omegon-maintain",
                f"echo '{identity(exclusions=MAINTENANCE_EXCLUSIONS[:-1])}'",
            )
            with self.assertRaisesRegex(RuntimeError, "unexpected exclusion set"):
                validate(root / "omegon", root / "omegon-maintain")


if __name__ == "__main__":
    unittest.main()
