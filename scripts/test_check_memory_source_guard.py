#!/usr/bin/env python3
"""Tests for the managed-memory source guard."""

from __future__ import annotations

import importlib.util
import sys
import tempfile
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).with_name("check_memory_source_guard.py")
SPEC = importlib.util.spec_from_file_location("check_memory_source_guard", MODULE_PATH)
assert SPEC and SPEC.loader
guard = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = guard
SPEC.loader.exec_module(guard)


class MemorySourceGuardTests(unittest.TestCase):
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

    def test_rejects_backend_alias_and_live_sqlite_open(self) -> None:
        self.write(
            "consumer.rs",
            "use omegon_memory::MemoryBackend as DurableStore;\n"
            "fn bypass(path: &Path) { let _ = SqliteBackend::open_existing(path); }\n",
        )
        policies = self.policies()
        self.assertIn("forbidden direct MemoryBackend ownership or import alias", policies)
        self.assertIn("forbidden direct SqliteBackend live open", policies)

    def test_rejects_sqlite_connection_vault_and_spawn_aliases(self) -> None:
        self.write(
            "consumer.rs",
            "use omegon_memory::SqliteBackend as Store;\n"
            "use rusqlite::Connection as Db;\n"
            "use omegon_memory::vault_sync as sync;\n"
            "use tokio::task as jobs;\n"
            "fn bypass(root: &Path, binding: Binding) {\n"
            " let db_path = root.join(\"facts.db\");\n"
            " let _ = Store::open(&db_path);\n"
            " let _ = Db::open(&db_path);\n"
            " let _ = sync::materialize_to_vault(store, root, \"mind\");\n"
            " jobs::spawn(async move { binding.invoke(MemoryRequestV1::VaultSessionEnd { mind }).await; });\n"
            "}\n",
        )
        policies = self.policies()
        self.assertIn("forbidden direct SqliteBackend live open", policies)
        self.assertIn("forbidden memory-path rusqlite open", policies)
        self.assertIn("forbidden direct vault synchronization API", policies)
        self.assertIn("forbidden detached memory persistence task", policies)

    def test_rejects_task_qualified_and_function_aliased_spawn(self) -> None:
        self.write(
            "consumer.rs",
            "use tokio::task::spawn as launch;\n"
            "fn one(binding: Binding) { tokio::task::spawn(async move { backend.store_fact(fact).await; }); }\n"
            "fn two(binding: Binding) { launch(async move { backend.store_embedding(fact).await; }); }\n",
        )
        policies = self.policies()
        self.assertEqual(policies.count("forbidden detached memory persistence task"), 2)
        self.assertEqual(policies.count("forbidden direct backend persistence method"), 2)

    def test_rejects_memory_rusqlite_jsonl_vault_and_canonical_writes(self) -> None:
        self.write(
            "consumer.rs",
            "fn bypass(root: &Path, backend: &B) {\n"
            " let db = root.join(\"ai/memory/facts.db\");\n"
            " let _ = rusqlite::Connection::open(db);\n"
            " let _ = backend.export_jsonl(\"mind\");\n"
            " let _ = vault_sync::materialize_to_vault(backend, root, \"mind\");\n"
            " std::fs::write(root.join(\"facts.jsonl\"), body);\n"
            " std::fs::rename(project_jsonl_path, replacement);\n"
            "}\n",
        )
        policies = self.policies()
        self.assertTrue(any("rusqlite" in policy for policy in policies))
        self.assertTrue(any("JSONL" in policy for policy in policies))
        self.assertTrue(any("vault" in policy for policy in policies))
        self.assertEqual(sum("canonical" in policy for policy in policies), 2)

    def test_rejects_detached_persistence_task(self) -> None:
        self.write(
            "consumer.rs",
            "fn detach(binding: Binding) { tokio::spawn(async move {\n"
            " binding.invoke(MemoryRequestV1::VaultSessionEnd { mind }).await;\n"
            " }); }\n",
        )
        self.assertIn("forbidden detached memory persistence task", self.policies())

    def test_rejects_backend_persistence_methods(self) -> None:
        self.write(
            "consumer.rs",
            "fn bypass(backend: &Store) {\n"
            " backend.store_fact(fact);\n"
            " backend.store_embedding(fact, vector);\n"
            " backend.store_episode(episode);\n"
            " backend.create_edge(edge);\n"
            " backend.import_jsonl(data);\n"
            " backend.export_jsonl(\"mind\");\n"
            "}\n",
        )
        policies = self.policies()
        self.assertEqual(policies.count("forbidden direct backend persistence method"), 4)
        self.assertEqual(policies.count("forbidden direct JSONL import/export"), 2)

    def test_masks_comments_strings_raw_strings_and_cfg_test_items(self) -> None:
        self.write(
            "consumer.rs",
            "// MemoryBackend SqliteBackend::open(path)\n"
            "const DOC: &str = r#\"backend.export_jsonl(\"mind\")\"#;\n"
            "const QUOTE: char = '\\'';\n"
            "const SLASH: u8 = b'\\\\';\n"
            "const TEXT: char = 'S'; // SqliteBackend::open(path)\n"
            "#[cfg(test)] fn fixture() { let _ = SqliteBackend::open(\"facts.db\"); }\n"
            "#[cfg(test)] mod tests { fn fixture() { vault_sync::import_from_vault(); } }\n",
        )
        self.assertEqual(self.policies(), [])

    def test_only_exact_cfg_test_items_are_masked(self) -> None:
        self.write(
            "consumer.rs",
            "#[cfg(not(test))] fn not_test() { let _ = SqliteBackend::open(\"facts.db\"); }\n"
            "#[cfg(any(test, feature = \"live\"))] fn maybe_live() { backend.store_fact(fact); }\n"
            "#[cfg(all(test, feature = \"extra\"))] fn compound() { backend.export_jsonl(\"mind\"); }\n",
        )
        policies = self.policies()
        self.assertIn("forbidden direct SqliteBackend live open", policies)
        self.assertIn("forbidden direct backend persistence method", policies)
        self.assertIn("forbidden direct JSONL import/export", policies)

    def test_cfg_test_variant_does_not_consume_following_production_function(self) -> None:
        self.write(
            "consumer.rs",
            "enum Request { #[cfg(test)] TestOnly, Production }\n"
            "fn production() { let _ = SqliteBackend::open(\"facts.db\"); }\n",
        )
        self.assertEqual(self.policies(), ["forbidden direct SqliteBackend live open"])

    def test_cfg_test_field_and_match_arm_do_not_consume_following_code(self) -> None:
        self.write(
            "consumer.rs",
            "struct S { #[cfg(test)] test: bool, production: bool }\n"
            "fn production(value: E) { match value { #[cfg(test)] E::Test => (), _ => () }; backend.store_fact(fact); }\n",
        )
        self.assertEqual(self.policies(), ["forbidden direct backend persistence method"])

    def test_allows_external_test_module_and_exact_owner_paths(self) -> None:
        self.write("lib.rs", "#[cfg(test)] mod campaign;\n")
        self.write("campaign.rs", "fn fixture() { let _ = SqliteBackend::open(\"facts.db\"); }\n")
        self.write("memory_service.rs", "fn owner() { let _ = SqliteBackend::open(\"facts.db\"); }\n")
        self.write("migrate.rs", "fn migrate() { let _ = SqliteBackend::open(\"facts.db\"); }\n")
        self.assertEqual(self.policies(), [])

    def test_string_bait_cannot_hide_a_production_file_as_an_external_test_module(self) -> None:
        self.write(
            "lib.rs",
            'const BAIT: &str = "#[cfg(test)] mod production;";\n',
        )
        self.write(
            "production.rs",
            'fn owner() { let _ = SqliteBackend::open("facts.db"); }\n',
        )
        self.assertEqual(self.policies(), ["forbidden direct SqliteBackend live open"])

    def test_allows_exact_extension_mind_authority_but_rejects_sibling_owner(self) -> None:
        owner = 'fn save(root: &Path) { std::fs::write(root.join("facts.jsonl"), body); }\n'
        self.write("extensions/mind.rs", owner)
        self.write("extensions/sibling.rs", owner)
        violations = guard.scan(self.root)
        self.assertEqual(len(violations), 1)
        self.assertEqual(
            violations[0].path,
            guard.SOURCE_ROOT / "extensions/sibling.rs",
        )
        self.assertEqual(
            violations[0].policy,
            "forbidden canonical memory file mutation",
        )

    def test_setup_exclusion_is_function_scoped(self) -> None:
        self.write(
            "setup.rs",
            "fn ensure_project_memory_store_ready() { let _ = SqliteBackend::open(\"facts.db\"); }\n"
            "impl Other { fn ensure_project_memory_store_ready() { let _ = SqliteBackend::open(\"facts.db\"); } }\n"
            "fn bypass() { let _ = SqliteBackend::open(\"facts.db\"); }\n",
        )
        self.assertEqual(self.policies().count("forbidden direct SqliteBackend live open"), 2)

    def test_function_scoped_path_taint_reaches_sync_async_and_aliased_mutations(self) -> None:
        self.write(
            "consumer.rs",
            "use std::fs as disk;\n"
            "use tokio::fs as async_disk;\n"
            "use std::fs::remove_file as unlink;\n"
            "fn mutate(root: &Path) {\n"
            " let jsonl = root.join(\"facts.jsonl\");\n"
            " let moved = jsonl.clone();\n"
            " disk::write(&jsonl, body);\n"
            " async_disk::rename(&moved, root.join(\"old\"));\n"
            " unlink(moved);\n"
            "}\n",
        )
        self.assertEqual(self.policies().count("forbidden canonical memory file mutation"), 3)

    def test_function_scoped_path_taint_reaches_connection_alias(self) -> None:
        self.write(
            "consumer.rs",
            "use rusqlite::Connection as Db;\n"
            "fn open(root: &Path) { let path = root.join(\"facts.db\"); let alias = path.clone(); let _ = Db::open(alias); }\n",
        )
        self.assertEqual(self.policies(), ["forbidden memory-path rusqlite open"])

    def test_grouped_domain_import_aliases_are_rejected(self) -> None:
        self.write(
            "consumer.rs",
            "use omegon_memory::{SqliteBackend as Store, vault_sync as sync, Fact};\n"
            "use rusqlite::{Connection as Db, params};\n"
            "fn bypass(root: &Path) {\n"
            ' let path = root.join("facts.db");\n'
            " let _ = Store::open(&path);\n"
            " let _ = Db::open(&path);\n"
            ' let _ = sync::materialize_to_vault(store, root, "mind");\n'
            "}\n",
        )
        policies = self.policies()
        self.assertIn("forbidden direct SqliteBackend live open", policies)
        self.assertIn("forbidden memory-path rusqlite open", policies)
        self.assertIn("forbidden direct vault synchronization API", policies)

    def test_grouped_tokio_task_and_fs_aliases_are_rejected(self) -> None:
        self.write(
            "consumer.rs",
            "use tokio::{task as jobs, fs as async_disk};\n"
            "use tokio::task::{spawn as launch, JoinHandle};\n"
            "use std::{fs as disk, path::Path};\n"
            "use std::fs::{remove_file as unlink, write};\n"
            "fn bypass(root: &Path) {\n"
            ' let path = root.join("facts.jsonl");\n'
            " disk::write(&path, body);\n"
            " async_disk::rename(&path, old);\n"
            " unlink(&path);\n"
            " write(&path, body);\n"
            " jobs::spawn(async move { backend.store_fact(fact).await; });\n"
            " launch(async move { backend.store_episode(episode).await; });\n"
            "}\n",
        )
        policies = self.policies()
        self.assertEqual(policies.count("forbidden canonical memory file mutation"), 4)
        self.assertEqual(policies.count("forbidden detached memory persistence task"), 2)

    def test_tuple_destructuring_propagates_taint_positionally(self) -> None:
        self.write(
            "consumer.rs",
            "fn mutate(root: &Path, safe_one: &Path, safe_two: &Path) {\n"
            ' let canonical_path = root.join("facts.jsonl");\n'
            " let (alias,) = (canonical_path.clone(),);\n"
            " let (first, second) = (canonical_path.clone(), canonical_path);\n"
            " let (safe_alias, other_safe) = (safe_one, safe_two);\n"
            " std::fs::write(alias, body);\n"
            " std::fs::rename(first, second);\n"
            " std::fs::write(safe_alias, body);\n"
            " std::fs::remove_file(other_safe);\n"
            "}\n",
        )
        self.assertEqual(
            self.policies().count("forbidden canonical memory file mutation"),
            2,
        )

    def test_path_taint_does_not_cross_functions_or_unrelated_names(self) -> None:
        self.write(
            "consumer.rs",
            "fn source(root: &Path) { let path = root.join(\"facts.jsonl\"); consume(path); }\n"
            "fn unrelated(path: &Path) { std::fs::write(path, body); let _ = rusqlite::Connection::open(path); }\n",
        )
        self.assertEqual(self.policies(), [])


if __name__ == "__main__":
    unittest.main()
