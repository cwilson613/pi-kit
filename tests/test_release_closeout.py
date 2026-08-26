import importlib.util
import json
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts/check_release_closeout.py"
SPEC = importlib.util.spec_from_file_location("check_release_closeout", SCRIPT)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(MODULE)


class ReleaseCloseoutTests(unittest.TestCase):
    def test_checked_evidence_is_complete(self) -> None:
        evidence = MODULE.load_and_validate()
        self.assertEqual(evidence["release_signing_evidence"]["claim"], "checked_fixture_and_tests_only")
        self.assertEqual(len(evidence["maintenance_cases"]), 10)

    def test_live_signing_claim_is_rejected(self) -> None:
        evidence = json.loads(MODULE.EVIDENCE.read_text())
        evidence["release_signing_evidence"]["claim"] = "live_release_signed"
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "evidence.json"
            path.write_text(json.dumps(evidence))
            with self.assertRaisesRegex(ValueError, "must not claim live signing"):
                MODULE.load_and_validate(path)


if __name__ == "__main__":
    unittest.main()
