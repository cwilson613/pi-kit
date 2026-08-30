import subprocess
import unittest
from pathlib import Path
from unittest import mock

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "release_status.py"


class ReleaseStatusTests(unittest.TestCase):
    def test_detects_developer_id_signature(self) -> None:
        from scripts.release_status import detect_signing_status

        output = """
        Executable=/tmp/omegon
        Authority=Developer ID Application: Example (ABCDE12345)
        Authority=Developer ID Certification Authority
        Authority=Apple Root CA
        """
        self.assertEqual(detect_signing_status(output), "Developer ID (YubiKey)")

    def test_detects_self_signed_identity(self) -> None:
        from scripts.release_status import detect_signing_status

        output = "Authority=Omegon Local Dev\n"
        self.assertEqual(detect_signing_status(output), "Omegon Local Dev (self-signed)")

    def test_detects_adhoc_signature(self) -> None:
        from scripts.release_status import detect_signing_status

        output = "Signature=adhoc\n"
        self.assertEqual(detect_signing_status(output), "ad-hoc")

    def test_falls_back_to_unsigned(self) -> None:
        from scripts.release_status import detect_signing_status

        self.assertEqual(detect_signing_status("Identifier=omegon\n"), "unsigned")

    def test_missing_codesign_is_unsigned(self) -> None:
        from scripts.release_status import read_codesign_output

        with mock.patch("subprocess.run", side_effect=FileNotFoundError):
            self.assertEqual(read_codesign_output(Path("/missing/omegon")), "")

    def test_cli_prints_status(self) -> None:
        result = subprocess.run(
            ["python3", str(SCRIPT), "--binary", "/definitely/missing/binary"],
            cwd=ROOT,
            check=False,
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 0)
        self.assertEqual(result.stdout.strip(), "unsigned")


if __name__ == "__main__":
    unittest.main()
