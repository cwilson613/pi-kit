#!/usr/bin/env python3
"""
WSL Workspace Diagnostic Tool
Diagnoses Windows-mounted workspaces under WSL and reports performance,
case sensitivity, permission, and file-watching limitations.
"""

import os
import sys
import stat
import json
import pathlib
import platform
import tempfile
from dataclasses import dataclass, asdict
from enum import Enum
from typing import Dict, List, Optional, Tuple


class Severity(str, Enum):
    OK = "OK"
    INFO = "INFO"
    WARNING = "WARNING"
    CRITICAL = "CRITICAL"


@dataclass
class Issue:
    code: str
    severity: Severity
    summary: str
    description: str
    recommendation: str


@dataclass
class DiagnosticReport:
    workspace_path: str
    is_wsl: bool
    wsl_version: Optional[int]
    filesystem_type: str
    is_windows_mount: bool
    mount_point: str
    mount_options: List[str]
    case_sensitive: bool
    supports_chmod: bool
    supports_symlinks: bool
    issues: List[Dict]
    performance_score: str  # "OPTIMAL" | "SUBOPTIMAL" | "POOR"


class WSLWorkspaceDiagnoser:
    """Diagnoses WSL environment and filesystem behavior for a target workspace path."""

    def __init__(self, workspace_path: str):
        self.workspace_path = pathlib.Path(workspace_path).resolve()
        self.issues: List[Issue] = []

    def is_wsl(self) -> Tuple[bool, Optional[int]]:
        """Detect if running inside WSL and determine WSL version if possible."""
        osrelease_path = pathlib.Path("/proc/sys/kernel/osrelease")
        version_path = pathlib.Path("/proc/version")

        content = ""
        if osrelease_path.exists():
            content += osrelease_path.read_text().lower()
        if version_path.exists():
            content += version_path.read_text().lower()

        if "microsoft" not in content and "wsl" not in content:
            return False, None

        wsl_version = 2 if "wsl2" in content or "microsoft-standard" in content else 1
        return True, wsl_version

    def _get_mount_info(self) -> Tuple[str, str, List[str]]:
        """
        Parses /proc/mounts to find filesystem type, mount point, and options for workspace.
        Returns: (fs_type, mount_point, options_list)
        """
        mounts_file = pathlib.Path("/proc/mounts")
        if not mounts_file.exists():
            return "unknown", "/", []

        best_match_len = -1
        best_fs_type = "unknown"
        best_mount_point = "/"
        best_options = []

        try:
            with open(mounts_file, "r", encoding="utf-8") as f:
                for line in f:
                    parts = line.strip().split()
                    if len(parts) >= 4:
                        device, mount_pt, fs_type, opts = parts[:4]
                        # Check if workspace path is under this mount point
                        try:
                            rel = self.workspace_path.relative_to(pathlib.Path(mount_pt))
                            match_len = len(pathlib.Path(mount_pt).parts)
                            if match_len > best_match_len:
                                best_match_len = match_len
                                best_fs_type = fs_type
                                best_mount_point = mount_pt
                                best_options = opts.split(",")
                        except ValueError:
                            continue
        except Exception:
            pass

        return best_fs_type, best_mount_point, best_options

    def _check_case_sensitivity(self) -> bool:
        """Tests whether the workspace directory path is case sensitive."""
        target_dir = self.workspace_path if self.workspace_path.is_dir() else self.workspace_path.parent
        try:
            test_file_upper = target_dir / ".wsl_diag_case_TEST.tmp"
            test_file_lower = target_dir / ".wsl_diag_case_test.tmp"

            test_file_upper.unlink(missing_ok=True)
            test_file_lower.unlink(missing_ok=True)

            test_file_upper.touch()
            # If lower exists when upper touched, it's case insensitive
            is_case_sensitive = not test_file_lower.exists()

            test_file_upper.unlink(missing_ok=True)
            return is_case_sensitive
        except Exception:
            # Default fallback for read-only or restricted paths
            return not str(target_dir).startswith("/mnt/")

    def _check_chmod_support(self) -> bool:
        """Tests if Linux executable permission bits can be changed."""
        target_dir = self.workspace_path if self.workspace_path.is_dir() else self.workspace_path.parent
        test_file = target_dir / ".wsl_diag_chmod.tmp"
        try:
            test_file.unlink(missing_ok=True)
            test_file.touch()

            os.chmod(test_file, 0o644)
            mode1 = os.stat(test_file).st_mode

            os.chmod(test_file, 0o755)
            mode2 = os.stat(test_file).st_mode

            test_file.unlink(missing_ok=True)
            return (mode1 != mode2) and bool(mode2 & stat.S_IXUSR)
        except Exception:
            test_file.unlink(missing_ok=True)
            return False

    def _check_symlink_support(self) -> bool:
        """Tests if symbolic link creation is supported in workspace."""
        target_dir = self.workspace_path if self.workspace_path.is_dir() else self.workspace_path.parent
        target_file = target_dir / ".wsl_diag_symlink_target.tmp"
        link_file = target_dir / ".wsl_diag_symlink_link.tmp"

        try:
            target_file.unlink(missing_ok=True)
            link_file.unlink(missing_ok=True)

            target_file.touch()
            os.symlink(target_file.name, link_file)

            is_valid = link_file.is_symlink()

            target_file.unlink(missing_ok=True)
            link_file.unlink(missing_ok=True)
            return is_valid
        except Exception:
            target_file.unlink(missing_ok=True)
            link_file.unlink(missing_ok=True)
            return False

    def run_diagnostics(self) -> DiagnosticReport:
        is_wsl_env, wsl_ver = self.is_wsl()
        fs_type, mount_point, mount_options = self._get_mount_info()

        is_win_mount = (
            fs_type in ("drvfs", "9p", "cifs", "vboxsf")
            or str(self.workspace_path).startswith("/mnt/")
        )

        case_sensitive = self._check_case_sensitivity()
        supports_chmod = self._check_chmod_support()
        supports_symlinks = self._check_symlink_support()

        if not is_wsl_env or not is_win_mount:
            perf_score = "OPTIMAL"
        elif wsl_ver == 2:
            perf_score = "POOR"  # 9P filesystem bridge is slow across WSL2/Windows host boundary
        else:
            perf_score = "SUBOPTIMAL"

        if is_wsl_env and is_win_mount:
            self.issues.append(
                Issue(
                    code="WSL_WIN_MOUNT_PERF",
                    severity=Severity.WARNING if wsl_ver == 1 else Severity.CRITICAL,
                    summary="Workspace is stored on a Windows-mounted filesystem",
                    description=(
                        f"Workspace path '{self.workspace_path}' is on a Windows drive mount ({fs_type}). "
                        "Cross-OS filesystem translation causes severe I/O overhead (git, builds, indexing)."
                    ),
                    recommendation=(
                        "Move workspace to the Linux native root filesystem "
                        "(e.g., '/home/<user>/projects/...') for up to 10x I/O performance gains."
                    ),
                )
            )

            if not case_sensitive:
                self.issues.append(
                    Issue(
                        code="WSL_CASE_INSENSITIVE",
                        severity=Severity.WARNING,
                        summary="Workspace filesystem is case-insensitive",
                        description=(
                            "Windows filesystems ignore letter case by default. Linux tools expecting "
                            "case sensitivity (e.g. 'File.txt' vs 'file.txt') may experience unexpected behavior."
                        ),
                        recommendation=(
                            "Enable per-directory case sensitivity via Windows WSL commands (`fsutil.exe file "
                            "setCaseSensitiveInfo <path> enable`) or move workspace to native Linux filesystem."
                        ),
                    )
                )

            if not supports_chmod or "metadata" not in mount_options:
                self.issues.append(
                    Issue(
                        code="WSL_MISSING_METADATA",
                        severity=Severity.WARNING,
                        summary="Windows mount lacks 'metadata' option or chmod support",
                        description=(
                            "Linux file permissions (chmod +x, chown) may not persist or be honored properly "
                            "on this Windows-mounted workspace."
                        ),
                        recommendation=(
                            "Add 'options = \"metadata\"' to /etc/wsl.conf under [automount] or remount drive with metadata option:\n"
                            "  sudo mount -t drvfs C: /mnt/c -o remount,metadata"
                        ),
                    )
                )

            # 4. Inotify / File Watching issue
            self.issues.append(
                Issue(
                    code="WSL_INOTIFY_LIMITATION",
                    severity=Severity.INFO,
                    summary="File watching (inotify) may not reflect host-side changes",
                    description=(
                        "Files updated on the Windows side inside /mnt/<drive> may not trigger "
                        "Linux inotify events reliably for background watchers or hot-reloading dev servers."
                    ),
                    recommendation=(
                        "Use native WSL filesystem paths or enable polling in file-watching tools."
                    ),
                )
            )

        return DiagnosticReport(
            workspace_path=str(self.workspace_path),
            is_wsl=is_wsl_env,
            wsl_version=wsl_ver,
            filesystem_type=fs_type,
            is_windows_mount=is_win_mount,
            mount_point=mount_point,
            mount_options=mount_options,
            case_sensitive=case_sensitive,
            supports_chmod=supports_chmod,
            supports_symlinks=supports_symlinks,
            issues=[asdict(i) for i in self.issues],
            performance_score=perf_score,
        )


def format_cli_report(report: DiagnosticReport) -> str:
    """Formats the diagnostic report into readable terminal output."""
    lines = [
        "=" * 60,
        "          WSL WORKSPACE DIAGNOSTIC REPORT",
        "=" * 60,
        f"Workspace Path  : {report.workspace_path}",
        f"WSL Environment : {'Yes (WSL' + str(report.wsl_version) + ')' if report.is_wsl else 'No'}",
        f"Filesystem Type : {report.filesystem_type}",
        f"Mount Point     : {report.mount_point}",
        f"Windows Mount   : {'Yes' if report.is_windows_mount else 'No'}",
        f"Performance Score: {report.performance_score}",
        "-" * 60,
        "FILESYSTEM CAPABILITIES:",
        f"  Case Sensitive : {'Yes' if report.case_sensitive else 'No (Warning)'}",
        f"  Chmod Support  : {'Yes' if report.supports_chmod else 'No (Warning)'}",
        f"  Symlink Support: {'Yes' if report.supports_symlinks else 'No (Warning)'}",
        "-" * 60,
    ]

    if not report.issues:
        lines.append("Status: OK - No issues detected for this workspace.")
    else:
        lines.append(f"DIAGNOSED ISSUES ({len(report.issues)}):")
        for idx, issue in enumerate(report.issues, 1):
            lines.extend([
                f"\n[{idx}] [{issue['severity']}] {issue['code']}: {issue['summary']}",
                f"    Detail        : {issue['description']}",
                f"    Recommendation: {issue['recommendation']}",
            ])

    lines.append("=" * 60)
    return "\n".join(lines)


def main():
    path = sys.argv[1] if len(sys.argv) > 1 else "."
    diagnoser = WSLWorkspaceDiagnoser(path)
    report = diagnoser.run_diagnostics()

    if "--json" in sys.argv:
        print(json.dumps(asdict(report), indent=2))
    else:
        print(format_cli_report(report))


if __name__ == "__main__":
    main()