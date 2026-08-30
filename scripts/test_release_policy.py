#!/usr/bin/env python3
"""Run the Python composition and release-policy test suite."""

from __future__ import annotations

import sys
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))


TEST_MODULES = (
    "tests.test_composition_budgets",
    "tests.test_composition_release_gates",
    "tests.test_content_pack_packaging",
    "tests.test_distribution_policy",
    "tests.test_distribution_runtime_smoke",
    "tests.test_nightly_cutoff_workflow",
    "tests.test_nightly_standard_release",
    "tests.test_optional_domain_isolation",
    "tests.test_release_closeout",
    "tests.test_release_manifest",
    "tests.test_release_policy_workflow",
    "tests.test_release_preflight",
    "tests.test_release_status",
    "tests.test_task_capsule_dependency_boundary",
    "tests.test_validate_companion",
    "tests.test_verify_homebrew_formula",
)


def main() -> int:
    suite = unittest.defaultTestLoader.loadTestsFromNames(TEST_MODULES)
    result = unittest.TextTestRunner(verbosity=2).run(suite)
    return 0 if result.wasSuccessful() else 1


if __name__ == "__main__":
    raise SystemExit(main())
