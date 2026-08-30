#!/usr/bin/env python3
"""Exercise installed distribution layouts without network access."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import check_composition_matrix


ROOT = Path(__file__).resolve().parents[1]


def install_homebrew_layout(extracted: Path, prefix: Path) -> Path:
    """Apply the destinations used by homebrew/Formula/omegon.rb."""
    bin_dir = prefix / "bin"
    composition = prefix / "share/omegon/composition"
    bin_dir.mkdir(parents=True)
    composition.mkdir(parents=True)
    for name in ("omegon", "omegon-maintain"):
        shutil.copy2(extracted / name, bin_dir / name)
        shutil.copy2(
            extracted / f"{name}.composition-lock.json",
            composition / f"{name}.composition-lock.json",
        )
    (bin_dir / "om").symlink_to("omegon")
    shutil.copytree(extracted / "share/omegon", prefix / "share/omegon", dirs_exist_ok=True)
    return prefix


def validate_host_only(payload: dict, metadata: dict, distribution: str) -> None:
    probe = payload.get("functional_probe", {})
    expected_metadata = {
        "schema_version": 1,
        "distribution": distribution,
        "host_profile": "full-product",
        "composition_class": "host-only",
        "core_components": [],
    }
    if metadata != expected_metadata:
        raise ValueError(f"{distribution} host-only metadata is not exact")
    if (
        payload.get("artifact_profile") != "full-product"
        or payload.get("external_processes") != []
        or probe.get("status") != "unavailable"
        or probe.get("code") != "service:unavailable"
        or probe.get("component_id") != "core:codescan"
    ):
        raise ValueError(f"{distribution} did not prove typed codescan absence with zero process")


def run_full_product(archive: Path, target: str, generation: Path, executable: Path) -> None:
    profile = check_composition_matrix.load_policy()["profiles"]["full"]
    evidence = check_composition_matrix.exercise_installed_full_product(
        archive, target, profile, generation, executable
    )
    if not evidence["rollback_valid"]:
        raise ValueError("installed full-product rollback restoration failed")


def smoke_direct(archive: Path, target: str, installer: Path) -> None:
    version = archive.name.removeprefix("omegon-").removesuffix(f"-{target}.tar.gz")
    with tempfile.TemporaryDirectory(prefix="omegon-direct-installer-") as directory:
        root = Path(directory)
        home = root / "home"
        install_dir = root / "bin"
        no_network = root / "no-network"
        home.mkdir()
        install_dir.mkdir()
        no_network.mkdir()
        curl = no_network / "curl"
        curl.write_text("#!/bin/sh\nexit 97\n")
        curl.chmod(0o755)
        checksums = root / "checksums.sha256"
        checksums.write_text(f"{hashlib.sha256(archive.read_bytes()).hexdigest()}  {archive.name}\n")
        env = os.environ.copy()
        env.update(
            {
                "HOME": str(home),
                "INSTALL_DIR": str(install_dir),
                "VERSION": f"v{version}",
                "NO_COLOR": "1",
                "OMEGON_INSTALL_ARCHIVE": str(archive.resolve()),
                "OMEGON_INSTALL_CHECKSUMS": str(checksums.resolve()),
                "PATH": f"{no_network}{os.pathsep}{env['PATH']}",
            }
        )
        subprocess.run(["sh", str(installer), "--no-confirm"], cwd=ROOT, env=env, check=True)
        current = (home / ".omegon/current").resolve()
        if (install_dir / "omegon").resolve() != current / "omegon":
            raise ValueError("direct installer launcher does not select the installed generation")
        run_full_product(archive, target, current, Path("omegon"))


def smoke_homebrew(archive: Path, target: str) -> None:
    extracted = check_composition_matrix.verify_archive_inventory(archive, target)
    try:
        with tempfile.TemporaryDirectory(prefix="omegon-homebrew-runtime-") as directory:
            prefix = install_homebrew_layout(extracted, Path(directory) / "prefix")
            run_full_product(archive, target, prefix, Path("bin/omegon"))
    finally:
        shutil.rmtree(extracted, ignore_errors=True)


def smoke_host(binary: Path, metadata_path: Path, distribution: str) -> None:
    with tempfile.TemporaryDirectory(prefix=f"omegon-{distribution}-host-") as directory:
        root = Path(directory)
        env = os.environ.copy()
        env.update({"HOME": str(root / "home"), "OMEGON_HOME": str(root / "home/.omegon"), "OMEGON_LOG": "error"})
        (root / "home/.omegon").mkdir(parents=True)
        result = subprocess.run(
            [str(binary), "composition-inspect", "--profile", "full", "--probe", "codescan-search", "--cwd", str(root)],
            cwd=root,
            env=env,
            check=True,
            capture_output=True,
            text=True,
            timeout=180,
        )
        validate_host_only(json.loads(result.stdout), json.loads(metadata_path.read_text()), distribution)


def main() -> int:
    parser = argparse.ArgumentParser()
    subparsers = parser.add_subparsers(dest="mode", required=True)
    for mode in ("direct-installer", "homebrew"):
        command = subparsers.add_parser(mode)
        command.add_argument("--archive", type=Path, required=True)
        command.add_argument("--target", required=True)
        if mode == "direct-installer":
            command.add_argument("--installer", type=Path, default=ROOT / "core/install.sh")
    host = subparsers.add_parser("nix-host")
    host.add_argument("--binary", type=Path, required=True)
    host.add_argument("--metadata", type=Path, required=True)
    payload = subparsers.add_parser("oci-host")
    payload.add_argument("--payload", type=Path, required=True)
    payload.add_argument("--metadata", type=Path, required=True)
    args = parser.parse_args()
    if args.mode == "direct-installer":
        smoke_direct(args.archive, args.target, args.installer)
    elif args.mode == "homebrew":
        smoke_homebrew(args.archive, args.target)
    elif args.mode == "nix-host":
        smoke_host(args.binary, args.metadata, "nix")
    else:
        validate_host_only(json.loads(args.payload.read_text()), json.loads(args.metadata.read_text()), "oci")
    print(f"{args.mode} distribution runtime smoke: ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
