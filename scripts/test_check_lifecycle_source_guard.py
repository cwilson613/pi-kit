#!/usr/bin/env python3
"""Tests for the lifecycle repository source guard."""

from __future__ import annotations

import importlib.util
import sys
import tempfile
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).with_name("check_lifecycle_source_guard.py")
SPEC = importlib.util.spec_from_file_location("check_lifecycle_source_guard", MODULE_PATH)
assert SPEC and SPEC.loader
guard = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = guard
SPEC.loader.exec_module(guard)


class LifecycleSourceGuardTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.src = self.root / guard.SOURCE_ROOT
        self.src.mkdir(parents=True)

    def tearDown(self) -> None:
        self.temp.cleanup()

    def write(self, relative: str, source: str) -> None:
        path = self.src / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(source, encoding="utf-8")

    def policies(self) -> list[str]:
        return [violation.policy for violation in guard.scan(self.root)]

    def test_rejects_direct_owner_construction(self) -> None:
        self.write(
            "consumer.rs",
            "fn bypass(repo: &Path) {\n"
            "    let store = JsonFileStore::new(repo);\n"
            "    let _fsm = OpsxLifecycle::load(store);\n"
            "    let _repo = OpenSpecRepository::new(repo);\n"
            "}\n",
        )
        policies = self.policies()
        self.assertEqual(len(policies), 3)
        self.assertTrue(all("construction" in policy for policy in policies))

    def test_rejects_direct_and_named_canonical_writes(self) -> None:
        self.write(
            "consumer.rs",
            "fn bypass(root: &Path) {\n"
            "    std::fs::write(root.join(\"openspec/changes/x/tasks.md\"), body);\n"
            "    let change_dir = root.join(\"ai/openspec/changes/y\");\n"
            "    let proposal = change_dir.join(\"proposal.md\");\n"
            "    atomic_write(&proposal, body);\n"
            "}\n",
        )
        policies = self.policies()
        self.assertEqual(policies.count("forbidden canonical lifecycle artifact write"), 2)

    def test_allows_inline_and_item_level_test_fixtures(self) -> None:
        self.write(
            "consumer.rs",
            "#[cfg(test)]\n"
            "fn fixture(repo: &Path) { let _ = JsonFileStore::new(repo); }\n"
            "#[cfg(test)]\n"
            "mod tests {\n"
            "    fn fixture(root: &Path) { std::fs::write(root.join(\"docs/node.md\"), \"x\"); }\n"
            "}\n"
            "fn clean() {}\n",
        )
        self.assertEqual(self.policies(), [])

    def test_allows_external_test_module_file(self) -> None:
        self.write("consumer.rs", "#[cfg(test)]\nmod fixtures;\nfn clean() {}\n")
        self.write("fixtures.rs", "fn fixture(repo: &Path) { let _ = Lifecycle::load(repo); }\n")
        self.assertEqual(self.policies(), [])

    def test_allows_exact_owner_and_frozen_exclusion_files(self) -> None:
        fixtures = {
            "lifecycle_service.rs": "fn owner(repo: &Path) { let _ = JsonFileStore::new(repo); }",
            "lifecycle_transaction.rs": "fn owner(repo: &Path) { let _ = Lifecycle::load(repo); }",
            "lifecycle/design.rs": "fn author(root: &Path) { std::fs::write(root.join(\"docs/node.md\"), \"x\"); }",
            "lifecycle/spec.rs": "fn author(root: &Path) { std::fs::write(root.join(\"openspec/changes/x/proposal.md\"), \"x\"); }",
            "tdd.rs": "fn evidence(root: &Path) { std::fs::write(root.join(\"openspec/changes/x/tasks.md\"), \"x\"); }",
            "lifecycle/codex_export.rs": "fn export(root: &Path) { std::fs::write(root.join(\"design/node.md\"), \"x\"); }",
            "migrate.rs": "fn migrate(root: &Path) { std::fs::write(root.join(\"docs/node.md\"), \"x\"); }",
        }
        for path, source in fixtures.items():
            self.write(path, source)
        self.assertEqual(self.policies(), [])

    def test_comments_and_strings_do_not_trigger_policy(self) -> None:
        self.write(
            "consumer.rs",
            "// JsonFileStore::new(repo)\n"
            "const EXAMPLE: &str = \"OpenSpecRepository::new(repo)\";\n",
        )
        self.assertEqual(self.policies(), [])


if __name__ == "__main__":
    unittest.main()
