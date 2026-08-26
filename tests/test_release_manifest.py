import hashlib
import io
import json
import subprocess
import tarfile
import tempfile
import textwrap
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "release_manifest.py"


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
                    "share/omegon/content-packs/omegon-shipped/content-pack.toml",
                ],
            )
            self.assertEqual(manifest["members"][0]["digest"], hashlib.sha256(b"agent").hexdigest())
            self.assertEqual(manifest["archive_filename"], archive.name)
            self.assertEqual(manifest["git_ref"], "refs/tags/v1.2.3")
            self.assertEqual(len(manifest["record_id"]), 64)
            self.assertEqual(len(manifest["composition_locks"]), 3)
            self.assertEqual(manifest["composition_locks"][2]["identity"], "content-pack:omegon-shipped")
            self.assertIsNone(manifest["composition_locks"][2]["resident_lock_path"])

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
