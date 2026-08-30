import importlib.util
import subprocess
import unittest
from pathlib import Path
from unittest.mock import patch


def load_checker():
    path = Path(__file__).resolve().parents[1] / "scripts/check_optional_domain_isolation.py"
    spec = importlib.util.spec_from_file_location("check_optional_domain_isolation", path)
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(module)
    return module


class OptionalDomainIsolationTests(unittest.TestCase):
    def test_codescan_proof_uses_native_extension_contracts(self) -> None:
        checker = load_checker()
        matrix = checker.load_matrix()
        codescan = next(domain for domain in matrix["domains"] if domain["id"] == "codescan")
        self.assertEqual(codescan["composition"], "release-coupled native extension")
        self.assertEqual(codescan["absence"]["test"], "absent_extension_is_typed_unavailable")
        self.assertEqual(
            codescan["degradation"]["test"],
            "unavailable_service_keeps_tools_declared_and_returns_typed_evidence",
        )
        self.assertNotIn("omegon-codescan", codescan["maintenance_packages"])

    def test_gate_executes_the_declared_test_command(self) -> None:
        checker = load_checker()
        matrix = checker.load_matrix()
        tests = sorted(
            {
                (entry["test"], entry.get("ignored", False))
                for domain in matrix["domains"]
                for entry in (domain["absence"], domain["degradation"])
            }
        )
        with patch.object(
            checker.subprocess,
            "run",
            return_value=subprocess.CompletedProcess([], 0),
        ) as run:
            self.assertEqual(checker.main(), 0)
        self.assertEqual(
            [call.args[0] for call in run.call_args_list],
            [
                [
                    *matrix["test_command_prefix"],
                    test,
                    *(["--", "--ignored"] if ignored else []),
                ]
                for test, ignored in tests
            ],
        )
        self.assertTrue(all(call.kwargs == {"cwd": checker.ROOT, "check": True} for call in run.call_args_list))


if __name__ == "__main__":
    unittest.main()
