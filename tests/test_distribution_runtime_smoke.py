import importlib.util
import json
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "smoke_distribution_runtime.py"


def load_smoke():
    spec = importlib.util.spec_from_file_location("smoke_distribution_runtime", SCRIPT)
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(module)
    return module


class DistributionRuntimeSmokeTests(unittest.TestCase):
    def test_homebrew_layout_uses_formula_install_destinations(self) -> None:
        smoke = load_smoke()
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            extracted = root / "archive"
            prefix = root / "prefix"
            (extracted / "share/omegon/components").mkdir(parents=True)
            for name in ("omegon", "omegon-maintain"):
                (extracted / name).write_text(name)
                (extracted / f"{name}.composition-lock.json").write_text("{}")
            (extracted / "share/omegon/components/core-codescan.lock.json").write_text("{}")

            generation = smoke.install_homebrew_layout(extracted, prefix)

            self.assertEqual(generation, prefix)
            self.assertTrue((prefix / "bin/omegon").is_file())
            self.assertTrue((prefix / "bin/omegon-maintain").is_file())
            self.assertTrue((prefix / "bin/om").is_symlink())
            self.assertTrue((prefix / "share/omegon/composition/omegon.composition-lock.json").is_file())
            self.assertTrue((prefix / "share/omegon/components/core-codescan.lock.json").is_file())

    def test_host_only_payload_requires_typed_absence_and_zero_processes(self) -> None:
        smoke = load_smoke()
        payload = {
            "artifact_profile": "full-product",
            "external_processes": [],
            "functional_probe": {
                "status": "unavailable",
                "code": "service:unavailable",
                "component_id": "core:codescan",
            },
        }
        metadata = {
            "schema_version": 1,
            "distribution": "nix",
            "host_profile": "full-product",
            "composition_class": "host-only",
            "core_components": [],
        }
        smoke.validate_host_only(payload, metadata, "nix")
        for mutation in (
            lambda value: value["functional_probe"].update(code="service:disabled"),
            lambda value: value.update(external_processes=[{"pid": 1}]),
        ):
            malformed = json.loads(json.dumps(payload))
            mutation(malformed)
            with self.assertRaises(ValueError):
                smoke.validate_host_only(malformed, metadata, "nix")


if __name__ == "__main__":
    unittest.main()
