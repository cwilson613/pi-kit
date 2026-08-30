#!/usr/bin/env python3
"""Build a strict release archive containing the companion pair and content pack."""

from __future__ import annotations

import argparse
import gzip
import hashlib
import io
import json
import re
import tarfile
from pathlib import Path

import content_pack_manifest

TARGET_RE = re.compile(r"omegon-(?P<version>.+)-(?P<target>aarch64-apple-darwin|x86_64-apple-darwin|aarch64-unknown-linux-gnu|x86_64-unknown-linux-gnu|x86_64-unknown-linux-musl)\.tar\.gz$")
ISSUER = "https://token.actions.githubusercontent.com"
OMEGON_REQUIRED_RESIDENT_IDENTITIES = (
    "system:constitutional-kernel",
    "system:default-loop",
    "system:host-effects",
)
OMEGON_OPTIONAL_RESIDENT_IDENTITIES = (
    "feature:codescan-adapter",
    "feature:context-compaction",
    "feature:git",
    "feature:lifecycle",
    "feature:memory",
)
OMEGON_MAINTAIN_RESIDENT_IDENTITIES = ("system:maintenance-kernel",)
CODESCAN_PREFIX = "share/omegon/extensions/omegon-codescan/"
CODESCAN_MANIFEST = f"{CODESCAN_PREFIX}manifest.toml"
CODESCAN_EXECUTABLE = f"{CODESCAN_PREFIX}target/release/omegon-codescan"
CODESCAN_COMPONENT_LOCK = "share/omegon/components/core-codescan.lock.json"
CODESCAN_MEMBERS = (CODESCAN_MANIFEST, CODESCAN_EXECUTABLE, CODESCAN_COMPONENT_LOCK)


def digest(payload: bytes) -> str:
    return hashlib.sha256(payload).hexdigest()


def canonical_json(value: dict) -> bytes:
    return (json.dumps(value, separators=(",", ":"), sort_keys=True) + "\n").encode()


def resident_lock(
    identity: str,
    payload: bytes,
    target: str,
    workflow_identity: str,
    verification: str,
) -> bytes:
    executable_digest = digest(payload)
    if identity == "omegon-maintain":
        required = OMEGON_MAINTAIN_RESIDENT_IDENTITIES
        optional = ()
    else:
        required = OMEGON_REQUIRED_RESIDENT_IDENTITIES
        optional = OMEGON_OPTIONAL_RESIDENT_IDENTITIES
    contributions = [
        {
            "artifact_digest": executable_digest,
            "artifact_path": identity,
            "fallback": "fail_closed",
            "identity": name,
            "protocol_maximum": 1,
            "protocol_minimum": 1,
            "required": True,
            "state": "resident",
            "targets": [target],
        }
        for name in required
    ]
    contributions.extend(
        {
            "artifact_digest": executable_digest,
            "artifact_path": identity,
            "fallback": "typed_unavailable",
            "identity": name,
            "protocol_maximum": 1,
            "protocol_minimum": 1,
            "required": False,
            "state": "resident_optional",
            "targets": [target],
        }
        for name in optional
    )
    return canonical_json({
        "contributions": contributions,
        "executable_digest": executable_digest,
        "executable_identity": identity,
        "protocol_maximum": 1,
        "protocol_minimum": 1,
        "schema_version": 1,
        "signing_identity": {
            "issuer": ISSUER,
            "verification": verification,
            "workflow_identity": workflow_identity,
        },
        "target": target,
    })


def codescan_component_lock(
    manifest: bytes,
    executable: bytes,
    target: str,
    workflow_identity: str,
    verification: str,
) -> dict:
    return {
        "component_id": "core:codescan",
        "executable_digest": digest(executable),
        "executable_path": CODESCAN_EXECUTABLE,
        "fallback": "typed_unavailable",
        "manifest_digest": digest(manifest),
        "manifest_path": CODESCAN_MANIFEST,
        "protocol_maximum": 1,
        "protocol_minimum": 1,
        "protocol_version": 1,
        "schema_version": 1,
        "signing_identity": {
            "issuer": ISSUER,
            "verification": verification,
            "workflow_identity": workflow_identity,
        },
        "target": target,
        "wire_manifest_id": "omegon-codescan",
    }


def add_file(archive: tarfile.TarFile, name: str, payload: bytes, mode: int) -> None:
    member = tarfile.TarInfo(name)
    member.mode = mode
    member.mtime = 0
    member.uid = 0
    member.gid = 0
    member.uname = ""
    member.gname = ""
    member.size = len(payload)
    archive.addfile(member, io.BytesIO(payload))


def package(
    binary_dir: Path,
    output: Path,
    codescan_binary: Path | None = None,
    codescan_manifest: Path | None = None,
    without_codescan: bool = False,
) -> None:
    match = TARGET_RE.search(output.name)
    if match is None:
        raise ValueError("release archive filename does not identify a supported target")
    target = match.group("target")
    workflow_identity = (
        "https://github.com/styrene-lab/omegon/.github/workflows/release.yml@"
        f"refs/tags/v{match.group('version')}"
    )
    content_manifest = content_pack_manifest.render().encode()
    with output.open("wb") as raw:
        with gzip.GzipFile(filename="", mode="wb", fileobj=raw, mtime=0) as compressed:
            with tarfile.open(fileobj=compressed, mode="w", format=tarfile.USTAR_FORMAT) as archive:
                for binary in ("omegon", "omegon-maintain"):
                    payload = (binary_dir / binary).read_bytes()
                    add_file(archive, binary, payload, 0o755)
                    add_file(
                        archive,
                        f"{binary}.composition-lock.json",
                        resident_lock(
                            binary,
                            payload,
                            target,
                            workflow_identity,
                            "required",
                        ),
                        0o644,
                    )
                prefix = "share/omegon/content-packs/omegon-shipped"
                add_file(
                    archive,
                    f"{prefix}/content-pack.toml",
                    content_manifest,
                    0o644,
                )
                for asset in content_pack_manifest.assets():
                    path = str(asset["path"])
                    add_file(archive, f"{prefix}/{path}", (content_pack_manifest.ROOT / path).read_bytes(), 0o644)
                if (codescan_binary is None) != (codescan_manifest is None):
                    raise ValueError("codescan binary and manifest must be supplied together")
                if codescan_binary is None and not without_codescan:
                    raise ValueError(
                        "product archives require codescan; use --without-codescan only for test fixtures"
                    )
                if codescan_binary is not None and codescan_manifest is not None:
                    manifest_payload = codescan_manifest.read_bytes()
                    executable_payload = codescan_binary.read_bytes()
                    add_file(archive, CODESCAN_MANIFEST, manifest_payload, 0o644)
                    add_file(
                        archive,
                        CODESCAN_EXECUTABLE,
                        executable_payload,
                        0o755,
                    )
                    add_file(
                        archive,
                        CODESCAN_COMPONENT_LOCK,
                        canonical_json(
                            codescan_component_lock(
                                manifest_payload,
                                executable_payload,
                                target,
                                workflow_identity,
                                "required",
                            )
                        ),
                        0o644,
                    )


def write_resident_locks(binary_dir: Path, output_dir: Path, target: str) -> None:
    output_dir.mkdir(parents=True, exist_ok=True)
    for binary in ("omegon", "omegon-maintain"):
        payload = (binary_dir / binary).read_bytes()
        (output_dir / f"{binary}.composition-lock.json").write_bytes(
            resident_lock(
                binary,
                payload,
                target,
                "local:source-or-linked-build",
                "not_applicable",
            )
        )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary-dir", type=Path, required=True)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--lock-dir", type=Path)
    parser.add_argument("--target")
    parser.add_argument("--codescan-binary", type=Path)
    parser.add_argument("--codescan-manifest", type=Path)
    parser.add_argument("--without-codescan", action="store_true")
    args = parser.parse_args()
    if args.lock_dir:
        if not args.target:
            parser.error("--lock-dir requires --target")
        write_resident_locks(args.binary_dir, args.lock_dir, args.target)
    elif args.output:
        package(
            args.binary_dir,
            args.output,
            args.codescan_binary,
            args.codescan_manifest,
            args.without_codescan,
        )
    else:
        parser.error("one of --output or --lock-dir is required")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
