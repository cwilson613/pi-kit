#!/usr/bin/env python3
"""Build a strict release archive containing the companion pair and content pack."""

from __future__ import annotations

import argparse
import gzip
import io
import tarfile
from pathlib import Path

import content_pack_manifest


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
    with output.open("wb") as raw:
        with gzip.GzipFile(filename="", mode="wb", fileobj=raw, mtime=0) as compressed:
            with tarfile.open(fileobj=compressed, mode="w", format=tarfile.USTAR_FORMAT) as archive:
                for binary in ("omegon", "omegon-maintain"):
                    add_file(archive, binary, (binary_dir / binary).read_bytes(), 0o755)
                prefix = "share/omegon/content-packs/omegon-shipped"
                add_file(
                    archive,
                    f"{prefix}/content-pack.toml",
                    content_pack_manifest.render().encode(),
                    0o644,
                )
                for asset in content_pack_manifest.assets():
                    path = str(asset["path"])
                    add_file(archive, f"{prefix}/{path}", (content_pack_manifest.ROOT / path).read_bytes(), 0o644)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary-dir", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    package(args.binary_dir, args.output)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
