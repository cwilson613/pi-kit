import hashlib
import io
import json
import subprocess
import tarfile
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "verify_homebrew_formula.py"
TARGETS = (
    "aarch64-apple-darwin",
    "x86_64-apple-darwin",
    "aarch64-unknown-linux-gnu",
    "x86_64-unknown-linux-gnu",
)


def write_archive(
    path: Path,
    *,
    include_maintenance: bool = True,
    include_resident_locks: bool = True,
    include_content_pack: bool = True,
    include_codescan: bool = True,
) -> str:
    with tarfile.open(path, "w:gz") as package:
        members = [("omegon", b"agent")]
        if include_maintenance:
            members.append(("omegon-maintain", b"maintain"))
        if include_resident_locks:
            members.extend(
                (
                    ("omegon.composition-lock.json", b"{}"),
                    ("omegon-maintain.composition-lock.json", b"{}"),
                )
            )
        if include_content_pack:
            members.append(
                (
                    "share/omegon/content-packs/omegon-shipped/content-pack.toml",
                    b'id = "omegon-shipped"\n',
                )
            )
        if include_codescan:
            target = next(target for target in TARGETS if path.name.endswith(f"-{target}.tar.gz"))
            manifest = b'name = "omegon-codescan"\n'
            executable = b"codescan"
            component_lock = {
                "component_id": "core:codescan",
                "executable_digest": hashlib.sha256(executable).hexdigest(),
                "executable_path": "share/omegon/extensions/omegon-codescan/target/release/omegon-codescan",
                "fallback": "typed_unavailable",
                "manifest_digest": hashlib.sha256(manifest).hexdigest(),
                "manifest_path": "share/omegon/extensions/omegon-codescan/manifest.toml",
                "protocol_maximum": 1,
                "protocol_minimum": 1,
                "protocol_version": 1,
                "schema_version": 1,
                "signing_identity": {
                    "issuer": "https://token.actions.githubusercontent.com",
                    "verification": "required",
                    "workflow_identity": "https://github.com/styrene-lab/omegon/.github/workflows/release.yml@refs/tags/v1.2.3",
                },
                "target": target,
                "wire_manifest_id": "omegon-codescan",
            }
            members.extend(
                (
                    ("share/omegon/extensions/omegon-codescan/manifest.toml", manifest),
                    ("share/omegon/extensions/omegon-codescan/target/release/omegon-codescan", executable),
                    ("share/omegon/components/core-codescan.lock.json", (json.dumps(component_lock, separators=(",", ":"), sort_keys=True) + "\n").encode()),
                )
            )
        for name, payload in members:
            member = tarfile.TarInfo(name)
            member.mode = 0o755 if name in {
                "omegon",
                "omegon-maintain",
                "share/omegon/extensions/omegon-codescan/target/release/omegon-codescan",
            } else 0o644
            member.size = len(payload)
            package.addfile(member, io.BytesIO(payload))
    return hashlib.sha256(path.read_bytes()).hexdigest()


def write_formula(path: Path, checksums: dict[str, str]) -> None:
    blocks = "\n".join(
        f'  url "https://github.com/styrene-lab/omegon/releases/download/v#{{version}}/'
        f'omegon-#{{version}}-{target}.tar.gz"\n  sha256 "{checksums[target]}"'
        for target in TARGETS
    )
    path.write_text(
        "class Omegon < Formula\n"
        '  version "1.2.3"\n'
        f"{blocks}\n"
        "  def install\n"
        '    bin.install "omegon"\n'
        '    bin.install "omegon-maintain"\n'
        '    (share/"omegon/composition").install "omegon.composition-lock.json", "omegon-maintain.composition-lock.json"\n'
        '    share.install "share/omegon"\n'
        "  end\n"
        "end\n"
    )


class VerifyHomebrewFormulaTests(unittest.TestCase):
    def run_script(self, formula: Path, archive_dir: Path) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            ["python3", str(SCRIPT), "--formula", str(formula), "--archive-dir", str(archive_dir)],
            cwd=ROOT,
            check=False,
            capture_output=True,
            text=True,
        )

    def test_accepts_companion_and_content_pack_archives(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            tmp = Path(tmpdir)
            checksums = {
                target: write_archive(tmp / f"omegon-1.2.3-{target}.tar.gz")
                for target in TARGETS
            }
            formula = tmp / "omegon.rb"
            write_formula(formula, checksums)
            result = self.run_script(formula, tmp)
            self.assertEqual(result.returncode, 0, result.stderr)

    def test_rejects_archive_without_maintenance_companion(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            tmp = Path(tmpdir)
            checksums = {}
            for target in TARGETS:
                checksums[target] = write_archive(
                    tmp / f"omegon-1.2.3-{target}.tar.gz",
                    include_maintenance=target != TARGETS[0],
                )
            formula = tmp / "omegon.rb"
            write_formula(formula, checksums)
            result = self.run_script(formula, tmp)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("both root executables", result.stderr)

    def test_rejects_archive_without_content_pack(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            tmp = Path(tmpdir)
            checksums = {}
            for target in TARGETS:
                checksums[target] = write_archive(
                    tmp / f"omegon-1.2.3-{target}.tar.gz",
                    include_content_pack=target != TARGETS[0],
                )
            formula = tmp / "omegon.rb"
            write_formula(formula, checksums)
            result = self.run_script(formula, tmp)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("content-pack manifest", result.stderr)

    def test_rejects_archive_without_resident_locks(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            tmp = Path(tmpdir)
            checksums = {}
            for target in TARGETS:
                checksums[target] = write_archive(
                    tmp / f"omegon-1.2.3-{target}.tar.gz",
                    include_resident_locks=target != TARGETS[0],
                )
            formula = tmp / "omegon.rb"
            write_formula(formula, checksums)
            result = self.run_script(formula, tmp)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("resident locks", result.stderr)

    def test_rejects_archive_without_codescan_product_component(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            tmp = Path(tmpdir)
            checksums = {}
            for target in TARGETS:
                checksums[target] = write_archive(
                    tmp / f"omegon-1.2.3-{target}.tar.gz",
                    include_codescan=target != TARGETS[0],
                )
            formula = tmp / "omegon.rb"
            write_formula(formula, checksums)
            result = self.run_script(formula, tmp)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("core:codescan", result.stderr)

    def test_rejects_formula_that_does_not_install_content_pack(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            tmp = Path(tmpdir)
            checksums = {
                target: write_archive(tmp / f"omegon-1.2.3-{target}.tar.gz")
                for target in TARGETS
            }
            formula = tmp / "omegon.rb"
            write_formula(formula, checksums)
            formula.write_text(formula.read_text().replace('    share.install "share/omegon"\n', ""))
            result = self.run_script(formula, tmp)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn('share.install "share/omegon"', result.stderr)

    def test_rejects_formula_that_does_not_install_resident_locks(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            tmp = Path(tmpdir)
            checksums = {
                target: write_archive(tmp / f"omegon-1.2.3-{target}.tar.gz")
                for target in TARGETS
            }
            formula = tmp / "omegon.rb"
            write_formula(formula, checksums)
            formula.write_text(
                formula.read_text().replace(
                    '    (share/"omegon/composition").install "omegon.composition-lock.json", "omegon-maintain.composition-lock.json"\n',
                    "",
                )
            )
            result = self.run_script(formula, tmp)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn('share/"omegon/composition"', result.stderr)

    def test_rejects_noncanonical_formula_url(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            tmp = Path(tmpdir)
            checksums = {
                target: write_archive(tmp / f"omegon-1.2.3-{target}.tar.gz")
                for target in TARGETS
            }
            formula = tmp / "omegon.rb"
            write_formula(formula, checksums)
            formula.write_text(formula.read_text().replace("styrene-lab/omegon", "attacker/omegon", 1))
            result = self.run_script(formula, tmp)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("not canonical", result.stderr)


if __name__ == "__main__":
    unittest.main()
