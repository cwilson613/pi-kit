#!/usr/bin/env python3
"""Verify that a Homebrew formula references canonical dual-binary archives."""

from __future__ import annotations

import argparse
import re
import shutil
import sys
import tempfile
import urllib.request
from pathlib import Path

from release_manifest import FORMULA_TARGET_ORDER, build_package_manifest, sha256_file

REPOSITORY = "styrene-lab/omegon"
SHA256_RE = re.compile(r"[0-9a-f]{64}")


def parse_formula(path: Path) -> tuple[str, dict[str, tuple[str, str]]]:
    content = path.read_text()
    version_match = re.search(r'^\s*version\s+"([^"]+)"\s*$', content, re.MULTILINE)
    if version_match is None:
        raise ValueError("Homebrew formula has no version")
    version = version_match.group(1)
    if not re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+(?:[-.][0-9A-Za-z.-]+)?", version):
        raise ValueError(f"Invalid Homebrew formula version: {version}")

    pairs = re.findall(
        r'^\s*url\s+"([^"]+)"\s*\n\s*sha256\s+"([^"]+)"\s*$',
        content,
        re.MULTILINE,
    )
    if len(pairs) != len(FORMULA_TARGET_ORDER):
        raise ValueError("Homebrew formula must contain four URL/checksum pairs")

    assets: dict[str, tuple[str, str]] = {}
    for raw_url, checksum in pairs:
        url = raw_url.replace("#{version}", version)
        target = next(
            (target for target in FORMULA_TARGET_ORDER if url.endswith(f"-{target}.tar.gz")),
            None,
        )
        if target is None or target in assets:
            raise ValueError(f"Unexpected or duplicate Homebrew archive URL: {url}")
        expected = (
            f"https://github.com/{REPOSITORY}/releases/download/v{version}/"
            f"omegon-{version}-{target}.tar.gz"
        )
        if url != expected:
            raise ValueError(f"Homebrew archive URL is not canonical: {url}")
        if SHA256_RE.fullmatch(checksum) is None:
            raise ValueError(f"Invalid SHA-256 for {target}")
        assets[target] = (url, checksum)

    if set(assets) != set(FORMULA_TARGET_ORDER):
        raise ValueError("Homebrew formula does not cover every supported target")
    for required in ('bin.install "omegon"', 'bin.install "omegon-maintain"'):
        if required not in content:
            raise ValueError(f"Homebrew formula is missing {required}")
    return version, assets


def download(url: str, destination: Path) -> None:
    request = urllib.request.Request(url, headers={"User-Agent": "omegon-homebrew-verifier"})
    with urllib.request.urlopen(request, timeout=120) as response, destination.open("wb") as output:
        shutil.copyfileobj(response, output, length=1024 * 1024)


def verify_formula(formula: Path, archive_dir: Path | None = None) -> None:
    version, assets = parse_formula(formula)
    with tempfile.TemporaryDirectory(prefix="omegon-homebrew-") as temporary:
        workspace = Path(temporary)
        for target in FORMULA_TARGET_ORDER:
            url, expected_sha = assets[target]
            filename = f"omegon-{version}-{target}.tar.gz"
            archive = (archive_dir / filename) if archive_dir is not None else workspace / filename
            if archive_dir is None:
                download(url, archive)
            if not archive.is_file():
                raise ValueError(f"Homebrew archive is missing: {archive}")
            actual_sha = sha256_file(archive)
            if actual_sha != expected_sha:
                raise ValueError(
                    f"Homebrew archive checksum mismatch for {target}: "
                    f"expected {expected_sha}, got {actual_sha}"
                )
            build_package_manifest(
                archive=archive,
                tag=f"v{version}",
                target=target,
                repo=REPOSITORY,
                commit="0" * 40,
            )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--formula", type=Path, required=True)
    parser.add_argument("--archive-dir", type=Path)
    args = parser.parse_args()
    try:
        verify_formula(args.formula, args.archive_dir)
    except (OSError, ValueError) as error:
        print(f"Homebrew formula verification failed: {error}", file=sys.stderr)
        return 1
    print("Homebrew formula archives contain the verified Omegon companion pair.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
