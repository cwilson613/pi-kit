import importlib.util
import tomllib
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def load_checker():
    path = ROOT / "scripts" / "check_task_capsule_dependency_boundary.py"
    spec = importlib.util.spec_from_file_location("check_task_capsule_dependency_boundary", path)
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(module)
    return module


class TaskCapsuleDependencyBoundaryTests(unittest.TestCase):
    def test_every_forbidden_package_is_detected_by_name(self) -> None:
        checker = load_checker()
        for package in checker.FORBIDDEN:
            with self.subTest(package=package):
                found = checker.forbidden_packages(f"{package} v1.0.0\n")
                self.assertEqual(list(found), [package])

    def test_similar_names_do_not_trigger_false_positives(self) -> None:
        checker = load_checker()
        self.assertEqual(
            checker.forbidden_packages("image-helper v1.0.0\nratatui-extra v1.0.0\n"),
            {},
        )

    def test_direct_presentation_dependencies_remain_tui_owned(self) -> None:
        checker = load_checker()
        path = ROOT / "core/crates/omegon/Cargo.toml"
        manifest_text = path.read_text()
        self.assertEqual(checker.direct_tui_ownership_errors(tomllib.loads(manifest_text)), [])
        for package in checker.DIRECT_TUI_DEPENDENCIES:
            with self.subTest(package=package):
                mutated = tomllib.loads(manifest_text)
                mutated["features"]["tui"].remove(f"dep:{package}")
                self.assertTrue(checker.direct_tui_ownership_errors(mutated))


if __name__ == "__main__":
    unittest.main()
