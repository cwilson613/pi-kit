#!/usr/bin/env python3
"""Release packaging helpers.

Two responsibilities:
1. Generate a canonical release manifest from release artifacts.
2. Update the Homebrew formula from that manifest.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
import tarfile
from pathlib import Path
from typing import Any

TARGETS = (
    "aarch64-apple-darwin",
    "x86_64-apple-darwin",
    "aarch64-unknown-linux-gnu",
    "x86_64-unknown-linux-gnu",
    "x86_64-unknown-linux-musl",
)

FORMULA_TARGET_ORDER = TARGETS[:4]
PACKAGE_EXECUTABLES = ("omegon", "omegon-maintain")
CONTENT_PREFIX = "share/omegon/content-packs/omegon-shipped/"
CONTENT_MANIFEST = f"{CONTENT_PREFIX}content-pack.toml"
DOMAIN_PREFIX = b"omegon-maint-v1\0"
MAX_ARCHIVE_BYTES = 2 * 1024 * 1024 * 1024
MAX_MEMBER_BYTES = 1024 * 1024 * 1024
MAX_AGGREGATE_BYTES = 4 * 1024 * 1024 * 1024


def infer_channel(tag: str) -> str:
    if "-nightly." in tag:
        return "nightly"
    return "stable"


def parse_checksums(checksums_path: Path) -> dict[str, dict[str, str]]:
    assets: dict[str, dict[str, str]] = {}
    for raw_line in checksums_path.read_text().splitlines():
        line = raw_line.strip()
        if not line:
            continue
        parts = line.split()
        if len(parts) < 2:
            raise ValueError(f"Malformed checksum line: {raw_line!r}")
        sha256, filename = parts[0], parts[-1]
        archive_name = Path(filename).name
        target = next(
            (
                candidate
                for candidate in TARGETS
                if archive_name.endswith(f"-{candidate}.tar.gz")
                and not archive_name.startswith("omegon-browser-")
            ),
            None,
        )
        if target is None:
            continue
        assets[target] = {
            "target": target,
            "filename": archive_name,
            "sha256": sha256,
        }
    missing = [target for target in TARGETS if target not in assets]
    if missing:
        print(f"Note: checksums not yet available for: {', '.join(missing)}", file=sys.stderr)
    return assets


def build_manifest(
    *,
    tag: str,
    checksums_path: Path,
    repo: str,
    commit: str,
) -> dict[str, Any]:
    version = tag.removeprefix("v")
    channel = infer_channel(tag)
    assets = parse_checksums(checksums_path)
    release_base = f"https://github.com/{repo}/releases/download/{tag}"

    manifest_assets = []
    for target in TARGETS:
        if target not in assets:
            continue
        asset = assets[target]
        filename = asset["filename"]
        manifest_assets.append(
            {
                **asset,
                "url": f"{release_base}/{filename}",
                "signature_url": f"{release_base}/{filename}.sig",
                "certificate_url": f"{release_base}/{filename}.pem",
            }
        )

    return {
        "version": version,
        "tag": tag,
        "channel": channel,
        "commit": commit,
        "release_url": f"https://github.com/{repo}/releases/tag/{tag}",
        "checksums_url": f"{release_base}/checksums.sha256",
        "sbom_url": f"{release_base}/omegon-sbom.cdx.json",
        "third_party_notices_url": f"{release_base}/THIRD_PARTY_NOTICES.md",
        "assets": manifest_assets,
    }


def write_json(path: Path, data: dict[str, Any]) -> None:
    path.write_text(json.dumps(data, indent=2, sort_keys=False) + "\n")


def derive_package_record_id(archive_digest: str, target: str, version: str) -> str:
    digest = hashlib.sha256(DOMAIN_PREFIX)
    for field in (b"package", bytes.fromhex(archive_digest), target.encode(), version.encode()):
        digest.update(len(field).to_bytes(8, "big"))
        digest.update(field)
    return digest.hexdigest()


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def build_package_manifest(
    *, archive: Path, tag: str, target: str, repo: str, commit: str
) -> dict[str, Any]:
    if target not in TARGETS:
        raise ValueError(f"Unsupported package target: {target}")
    if not re.fullmatch(r"[0-9a-f]{40}", commit):
        raise ValueError("Package commit must be 40 lowercase hexadecimal characters")
    version = tag.removeprefix("v")
    expected_name = f"omegon-{version}-{target}.tar.gz"
    if archive.name != expected_name:
        raise ValueError(f"Package archive must be named {expected_name}")
    if archive.stat().st_size > MAX_ARCHIVE_BYTES:
        raise ValueError("Package archive exceeds the compressed-byte limit")

    members = []
    seen: set[str] = set()
    aggregate_size = 0
    with tarfile.open(archive, mode="r:gz") as package:
        for member in package:
            allowed = member.name in PACKAGE_EXECUTABLES or member.name.startswith(CONTENT_PREFIX)
            if not member.isfile() or not allowed or member.name in seen:
                raise ValueError(f"Invalid package archive member: {member.name}")
            expected_mode = 0o755 if member.name in PACKAGE_EXECUTABLES else 0o644
            if member.mode != expected_mode:
                raise ValueError(f"Package member {member.name} must have mode {expected_mode:04o}")
            aggregate_size += member.size
            if member.size > MAX_MEMBER_BYTES or aggregate_size > MAX_AGGREGATE_BYTES:
                raise ValueError("Package member exceeds the uncompressed-byte limit")
            stream = package.extractfile(member)
            if stream is None:
                raise ValueError(f"Cannot read package member: {member.name}")
            digest = hashlib.sha256()
            consumed = 0
            while consumed < member.size:
                block = stream.read(min(1024 * 1024, member.size - consumed))
                if not block:
                    break
                consumed += len(block)
                digest.update(block)
            if consumed != member.size or stream.read(1):
                raise ValueError(f"Package member size changed while reading: {member.name}")
            members.append(
                {
                    "path": member.name,
                    "mode": member.mode,
                    "size": member.size,
                    "digest": digest.hexdigest(),
                }
            )
            seen.add(member.name)
    if not set(PACKAGE_EXECUTABLES).issubset(seen) or CONTENT_MANIFEST not in seen:
        raise ValueError("Package archive must contain both root executables and the content-pack manifest")
    members.sort(key=lambda member: member["path"])

    archive_digest = sha256_file(archive)
    git_ref = f"refs/tags/{tag}"
    return {
        "archive_digest": archive_digest,
        "archive_filename": archive.name,
        "commit": commit,
        "git_ref": git_ref,
        "issuer": "https://token.actions.githubusercontent.com",
        "members": members,
        "record_id": derive_package_record_id(archive_digest, target, version),
        "record_kind": "package_manifest",
        "repository": repo,
        "schema_version": 1,
        "tag": tag,
        "target": target,
        "version": version,
        "workflow_identity": f"https://github.com/{repo}/.github/workflows/release.yml@{git_ref}",
    }


def write_canonical_json(path: Path, data: dict[str, Any]) -> None:
    path.write_text(json.dumps(data, separators=(",", ":"), sort_keys=True) + "\n")


def load_manifest(path: Path) -> dict[str, Any]:
    return json.loads(path.read_text())


def asset_sha_by_target(manifest: dict[str, Any]) -> dict[str, str]:
    assets = manifest.get("assets")
    if not isinstance(assets, list):
        raise ValueError("Manifest missing assets array")
    result: dict[str, str] = {}
    for asset in assets:
        if not isinstance(asset, dict):
            raise ValueError("Manifest asset must be an object")
        target = asset.get("target")
        sha256 = asset.get("sha256")
        if not isinstance(target, str) or not isinstance(sha256, str):
            raise ValueError("Manifest asset missing target or sha256")
        result[target] = sha256
    missing = [target for target in FORMULA_TARGET_ORDER if target not in result]
    if missing:
        raise ValueError(f"Manifest missing assets for targets: {', '.join(missing)}")
    return result


def update_homebrew_formula(*, manifest_path: Path, formula_path: Path) -> None:
    manifest = load_manifest(manifest_path)
    version = manifest.get("version")
    if not isinstance(version, str) or not version:
        raise ValueError("Manifest missing version")

    sha_by_target = asset_sha_by_target(manifest)
    content = formula_path.read_text()
    content = re.sub(r'version ".*"', f'version "{version}"', content, count=1)

    # Strip any deprecate! directive — version-specific deprecations must not
    # survive into the next stable formula update.
    content = re.sub(r'\n  deprecate! .*\n', '\n', content)

    replacement_shas = [sha_by_target[target] for target in FORMULA_TARGET_ORDER]
    sha_iter = iter(replacement_shas)

    def replace_sha(match: re.Match[str]) -> str:
        try:
            sha = next(sha_iter)
        except StopIteration as exc:
            raise ValueError("Formula has more sha256 entries than expected") from exc
        return f'sha256 "{sha}"'

    updated = re.sub(r'sha256 "(?:[A-Fa-f0-9]+|PLACEHOLDER)"', replace_sha, content)
    try:
        next(sha_iter)
    except StopIteration:
        pass
    else:
        raise ValueError("Formula has fewer sha256 entries than expected")

    if 'bin.install_symlink "omegon" => "om"' not in updated:
        updated = updated.replace(
            '    bin.install "omegon"\n',
            '    bin.install "omegon"\n    bin.install_symlink "omegon" => "om"\n',
            1,
        )

    formula_path.write_text(updated)



def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    subparsers = parser.add_subparsers(dest="command", required=True)

    generate = subparsers.add_parser("generate", help="Generate release-manifest.json")
    generate.add_argument("--tag", required=True)
    generate.add_argument("--checksums", type=Path, required=True)
    generate.add_argument("--output", type=Path, required=True)
    generate.add_argument("--repo", required=True)
    generate.add_argument("--commit", required=True)

    package = subparsers.add_parser(
        "generate-package", help="Generate a canonical PackageManifestV1"
    )
    package.add_argument("--archive", type=Path, required=True)
    package.add_argument("--tag", required=True)
    package.add_argument("--target", required=True)
    package.add_argument("--output", type=Path, required=True)
    package.add_argument("--repo", required=True)
    package.add_argument("--commit", required=True)

    homebrew = subparsers.add_parser("update-homebrew", help="Update Homebrew formula from manifest")
    homebrew.add_argument("--manifest", type=Path, required=True)
    homebrew.add_argument("--formula", type=Path, required=True)

    args = parser.parse_args(argv)

    try:
        if args.command == "generate":
            manifest = build_manifest(
                tag=args.tag,
                checksums_path=args.checksums,
                repo=args.repo,
                commit=args.commit,
            )
            write_json(args.output, manifest)
        elif args.command == "generate-package":
            manifest = build_package_manifest(
                archive=args.archive,
                tag=args.tag,
                target=args.target,
                repo=args.repo,
                commit=args.commit,
            )
            write_canonical_json(args.output, manifest)
        elif args.command == "update-homebrew":
            update_homebrew_formula(manifest_path=args.manifest, formula_path=args.formula)
        else:
            raise ValueError(f"Unknown command: {args.command}")
    except ValueError as err:
        print(f"error: {err}", file=sys.stderr)
        return 1

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
