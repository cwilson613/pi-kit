import hashlib
import io
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


def write_archive(path: Path, *, include_maintenance: bool = True) -> str:
    with tarfile.open(path, "w:gz") as package:
        members = [("omegon", b"agent")]
        if include_maintenance:
            members.append(("omegon-maintain", b"maintain"))
        for name, payload in members:
            member = tarfile.TarInfo(name)
            member.mode = 0o755
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

    def test_accepts_exact_dual_binary_archives(self) -> None:
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
