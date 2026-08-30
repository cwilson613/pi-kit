import hashlib
import importlib.util
import io
import json
import subprocess
import sys
import tarfile
import tempfile
import textwrap
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "release_manifest.py"


def load(name: str):
    path = ROOT / "scripts" / f"{name}.py"
    sys.path.insert(0, str(path.parent))
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(module)
    return module


class ReleaseManifestTests(unittest.TestCase):
    def run_script(self, *args: str) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            ["python3", str(SCRIPT), *args],
            cwd=ROOT,
            check=False,
            capture_output=True,
            text=True,
        )

    def test_generate_manifest_from_checksums(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            tmp = Path(tmpdir)
            checksums = tmp / "checksums.sha256"
            checksums.write_text(textwrap.dedent("""\
                a1 omegon-0.15.9-aarch64-apple-darwin.tar.gz
                ext1 omegon-browser-0.15.9-aarch64-apple-darwin.tar.gz
                b2 omegon-0.15.9-x86_64-apple-darwin.tar.gz
                c3 omegon-0.15.9-aarch64-unknown-linux-gnu.tar.gz
                d4 omegon-0.15.9-x86_64-unknown-linux-gnu.tar.gz
            """))
            output = tmp / "release-manifest.json"

            result = self.run_script(
                "generate",
                "--tag",
                "v0.15.9",
                "--checksums",
                str(checksums),
                "--output",
                str(output),
                "--repo",
                "styrene-lab/omegon",
                "--commit",
                "deadbeef",
            )

            self.assertEqual(result.returncode, 0, result.stderr)
            manifest = json.loads(output.read_text())
            self.assertEqual(manifest["version"], "0.15.9")
            self.assertEqual(manifest["tag"], "v0.15.9")
            self.assertEqual(manifest["channel"], "stable")
            self.assertEqual(manifest["commit"], "deadbeef")
            self.assertEqual(len(manifest["assets"]), 4)
            self.assertEqual(manifest["assets"][0]["sha256"], "a1")
            self.assertEqual(manifest["assets"][0]["host_profile"], "full-product")
            self.assertEqual(manifest["assets"][0]["composition_class"], "full-product")
            self.assertEqual(
                manifest["assets"][0]["core_components"],
                [{"component_id": "core:codescan", "wire_manifest_id": "omegon-codescan"}],
            )
            self.assertEqual(manifest["assets"][0]["sdk_extension_posture"], "operator-managed")
            self.assertEqual(
                manifest["assets"][0]["url"],
                "https://github.com/styrene-lab/omegon/releases/download/v0.15.9/omegon-0.15.9-aarch64-apple-darwin.tar.gz",
            )

    def test_update_homebrew_formula_from_manifest(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            tmp = Path(tmpdir)
            manifest = tmp / "release-manifest.json"
            manifest.write_text(json.dumps({
                "version": "1.2.3",
                "assets": [
                    {"target": "aarch64-apple-darwin", "sha256": "aa"},
                    {"target": "x86_64-apple-darwin", "sha256": "bb"},
                    {"target": "aarch64-unknown-linux-gnu", "sha256": "cc"},
                    {"target": "x86_64-unknown-linux-gnu", "sha256": "dd"},
                ],
            }))
            formula = tmp / "omegon.rb"
            formula.write_text(textwrap.dedent("""\
                class Omegon < Formula
                  version "0.0.1"
                  sha256 "1111"
                  sha256 "2222"
                  sha256 "3333"
                  sha256 "4444"
                end
            """))

            result = self.run_script(
                "update-homebrew",
                "--manifest",
                str(manifest),
                "--formula",
                str(formula),
            )

            self.assertEqual(result.returncode, 0, result.stderr)
            updated = formula.read_text()
            self.assertIn('version "1.2.3"', updated)
            self.assertIn('sha256 "aa"', updated)
            self.assertIn('sha256 "bb"', updated)
            self.assertIn('sha256 "cc"', updated)
            self.assertIn('sha256 "dd"', updated)

    def test_generate_canonical_package_manifest(self) -> None:
        package_release = load("package_release")
        with tempfile.TemporaryDirectory() as tmpdir:
            tmp = Path(tmpdir)
            archive = tmp / "omegon-1.2.3-x86_64-unknown-linux-gnu.tar.gz"
            with tarfile.open(archive, "w:gz") as package:
                for name, payload in (("omegon", b"agent"), ("omegon-maintain", b"maintain")):
                    member = tarfile.TarInfo(name)
                    member.mode = 0o755
                    member.size = len(payload)
                    package.addfile(member, io.BytesIO(payload))
                    lock_payload = b"{}\n"
                    lock = tarfile.TarInfo(f"{name}.composition-lock.json")
                    lock.mode = 0o644
                    lock.size = len(lock_payload)
                    package.addfile(lock, io.BytesIO(lock_payload))
                payload = b"schema_version = 1\n"
                member = tarfile.TarInfo("share/omegon/content-packs/omegon-shipped/content-pack.toml")
                member.mode = 0o644
                member.size = len(payload)
                package.addfile(member, io.BytesIO(payload))
                for name, payload, mode in (
                    ("share/omegon/extensions/omegon-codescan/manifest.toml", b'name = "omegon-codescan"\n', 0o644),
                    ("share/omegon/extensions/omegon-codescan/target/release/omegon-codescan", b"codescan", 0o755),
                ):
                    member = tarfile.TarInfo(name)
                    member.mode = mode
                    member.size = len(payload)
                    package.addfile(member, io.BytesIO(payload))
                lock = package_release.codescan_component_lock(
                    b'name = "omegon-codescan"\n',
                    b"codescan",
                    "x86_64-unknown-linux-gnu",
                    "https://github.com/styrene-lab/omegon/.github/workflows/release.yml@refs/tags/v1.2.3",
                    "required",
                )
                payload = package_release.canonical_json(lock)
                member = tarfile.TarInfo(package_release.CODESCAN_COMPONENT_LOCK)
                member.mode = 0o644
                member.size = len(payload)
                package.addfile(member, io.BytesIO(payload))
            output = tmp / "package-manifest.json"

            result = self.run_script(
                "generate-package",
                "--archive",
                str(archive),
                "--tag",
                "v1.2.3",
                "--target",
                "x86_64-unknown-linux-gnu",
                "--output",
                str(output),
                "--repo",
                "styrene-lab/omegon",
                "--commit",
                "a" * 40,
            )

            self.assertEqual(result.returncode, 0, result.stderr)
            raw = output.read_bytes()
            manifest = json.loads(raw)
            self.assertEqual(raw, json.dumps(manifest, separators=(",", ":"), sort_keys=True).encode() + b"\n")
            self.assertEqual(
                [member["path"] for member in manifest["members"]],
                [
                    "omegon",
                    "omegon-maintain",
                    "omegon-maintain.composition-lock.json",
                    "omegon.composition-lock.json",
                    "share/omegon/components/core-codescan.lock.json",
                    "share/omegon/content-packs/omegon-shipped/content-pack.toml",
                    "share/omegon/extensions/omegon-codescan/manifest.toml",
                    "share/omegon/extensions/omegon-codescan/target/release/omegon-codescan",
                ],
            )
            self.assertEqual(manifest["members"][0]["digest"], hashlib.sha256(b"agent").hexdigest())
            self.assertEqual(manifest["archive_filename"], archive.name)
            self.assertEqual(manifest["git_ref"], "refs/tags/v1.2.3")
            self.assertEqual(len(manifest["record_id"]), 64)
            self.assertEqual(len(manifest["composition_locks"]), 3)
            self.assertEqual(manifest["composition_locks"][2]["identity"], "content-pack:omegon-shipped")
            self.assertIsNone(manifest["composition_locks"][2]["resident_lock_path"])
            self.assertEqual(manifest["host_profile"], "full-product")
            self.assertEqual(manifest["composition_class"], "full-product")
            self.assertEqual(
                manifest["core_components"],
                [{"component_id": "core:codescan", "wire_manifest_id": "omegon-codescan"}],
            )
            component = manifest["product_component_locks"][0]
            self.assertEqual(component["component_id"], "core:codescan")
            self.assertEqual(component["wire_manifest_id"], "omegon-codescan")
            self.assertEqual(component["target"], "x86_64-unknown-linux-gnu")
            self.assertEqual(component["protocol_minimum"], 1)
            self.assertEqual(component["protocol_maximum"], 1)
            self.assertEqual(component["protocol_version"], 1)
            self.assertEqual(component["fallback"], "typed_unavailable")
            self.assertEqual(component["signing_identity"]["issuer"], package_release.ISSUER)
            self.assertEqual(
                component["signing_identity"]["workflow_identity"],
                manifest["workflow_identity"],
            )
            members = {member["path"]: member for member in manifest["members"]}
            self.assertEqual(component["manifest_digest"], members[component["manifest_path"]]["digest"])
            self.assertEqual(component["executable_digest"], members[component["executable_path"]]["digest"])
            self.assertIn(package_release.CODESCAN_COMPONENT_LOCK, members)
            self.assertEqual(manifest["sdk_extension_posture"], "operator-managed")

    def test_full_product_manifest_rejects_component_evidence_substitution(self) -> None:
        package_release = load("package_release")
        release_manifest = load("release_manifest")
        with tempfile.TemporaryDirectory() as tmpdir:
            tmp = Path(tmpdir)
            binaries = tmp / "bin"
            binaries.mkdir()
            for name in ("omegon", "omegon-maintain"):
                (binaries / name).write_bytes(name.encode())
            codescan = tmp / "omegon-codescan"
            codescan.write_bytes(b"codescan")
            archive = tmp / "omegon-1.2.3-x86_64-unknown-linux-gnu.tar.gz"
            package_release.package(
                binaries,
                archive,
                codescan,
                ROOT / "extensions/omegon-codescan/manifest.toml",
            )
            manifest = release_manifest.build_package_manifest(
                archive=archive,
                tag="v1.2.3",
                target="x86_64-unknown-linux-gnu",
                repo="styrene-lab/omegon",
                commit="a" * 40,
            )
            component = manifest["product_component_locks"][0]
            for field, value in (
                ("component_id", "executable:omegon"),
                ("wire_manifest_id", "sdk:self-promoted"),
                ("manifest_digest", "0" * 64),
                ("executable_digest", "0" * 64),
                ("target", "aarch64-apple-darwin"),
                ("protocol_version", 2),
            ):
                forged = json.loads(json.dumps(manifest))
                forged["product_component_locks"][0][field] = value
                with self.subTest(field=field), self.assertRaises(ValueError):
                    release_manifest.validate_product_component_evidence(forged)

    def test_package_manifest_rejects_archive_confusion(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            tmp = Path(tmpdir)
            archive = tmp / "omegon-1.2.3-x86_64-unknown-linux-gnu.tar.gz"
            with tarfile.open(archive, "w:gz") as package:
                member = tarfile.TarInfo("omegon")
                member.mode = 0o755
                member.size = 1
                package.addfile(member, io.BytesIO(b"x"))

            result = self.run_script(
                "generate-package",
                "--archive",
                str(archive),
                "--tag",
                "v1.2.3",
                "--target",
                "x86_64-unknown-linux-gnu",
                "--output",
                str(tmp / "package-manifest.json"),
                "--repo",
                "styrene-lab/omegon",
                "--commit",
                "a" * 40,
            )

            self.assertNotEqual(result.returncode, 0)
            self.assertIn("both root executables", result.stderr)


if __name__ == "__main__":
    unittest.main()
