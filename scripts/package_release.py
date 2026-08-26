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
        required = ("maintenance-kernel",)
        optional = ()
    else:
        required = ("constitutional-kernel", "default-loop", "host-effects")
        optional = ("codescan", "context-compaction", "git", "lifecycle", "memory")
    contributions = [
        {
            "artifact_digest": executable_digest,
            "artifact_path": identity,
            "fallback": "fail_closed",
            "identity": f"system:{name}",
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
            "identity": f"feature:{name}",
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


def package(binary_dir: Path, output: Path) -> None:
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
    args = parser.parse_args()
    if args.lock_dir:
        if not args.target:
            parser.error("--lock-dir requires --target")
        write_resident_locks(args.binary_dir, args.lock_dir, args.target)
    elif args.output:
        package(args.binary_dir, args.output)
    else:
        parser.error("one of --output or --lock-dir is required")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
