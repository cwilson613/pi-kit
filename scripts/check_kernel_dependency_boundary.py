#!/usr/bin/env python3
"""Enforce the selected kernel target's positive and negative dependency policy."""

from __future__ import annotations

import json
import subprocess
import sys
import tomllib
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
MANIFEST = ROOT / "core/crates/omegon/Cargo.toml"
POLICY = ROOT / "fixtures/release-composition-matrix-v1.json"
KERNEL_BIN = "omegon-kernel-host"
FORBIDDEN_PRODUCT_PACKAGES = {
    "flynt-models",
    "git2",
    "omegon-git",
    "omegon-memory",
    "omegon-opsx",
    "omegon-skills",
    "omegon-web",
    "rusqlite",
    "styrene-work-model",
    "styrene-work-runtime",
}


def package_names(output: str) -> set[str]:
    return {
        fields[0]
        for line in output.splitlines()
        if (fields := line.split()) and not line.startswith("[")
    }


def product_packages(output: str) -> list[str]:
    return sorted(package_names(output) & FORBIDDEN_PRODUCT_PACKAGES)


def declared_roots() -> set[str]:
    policy = json.loads(POLICY.read_text())
    return set(
        policy["artifact_rows"]["kernel-only"]["positive_boundary"]["dependency_roots"]
    )


def enabled_direct_roots(metadata: dict) -> set[str]:
    packages = {package["id"]: package for package in metadata["packages"]}
    omegon = next(
        package
        for package in metadata["packages"]
        if Path(package["manifest_path"]).resolve() == MANIFEST
    )
    node = next(node for node in metadata["resolve"]["nodes"] if node["id"] == omegon["id"])
    return {
        packages[dependency["pkg"]]["name"]
        for dependency in node["deps"]
        if any(
            kind.get("kind") in (None, "build")
            for kind in dependency.get("dep_kinds", [])
        )
    }


def direct_root_errors(declared: set[str], enabled: set[str]) -> list[str]:
    errors = []
    if undeclared := sorted(enabled - declared):
        errors.append("enabled direct roots are undeclared: " + ", ".join(undeclared))
    if disabled := sorted(declared - enabled):
        errors.append("declared direct roots are disabled: " + ", ".join(disabled))
    return errors


def manifest_errors(manifest: dict) -> list[str]:
    errors = []
    bins = {target["name"]: target for target in manifest.get("bin", [])}
    if bins.get(KERNEL_BIN, {}).get("required-features") != ["kernel-host"]:
        errors.append(f"{KERNEL_BIN} must require exactly kernel-host")
    if bins.get("omegon", {}).get("required-features") != ["product"]:
        errors.append("omegon product target must require exactly product")

    dependencies = manifest.get("dependencies", {})
    features = manifest.get("features", {})
    product_domains = set(features.get("product-domains", []))
    for package in sorted(FORBIDDEN_PRODUCT_PACKAGES):
        dependency = dependencies.get(package)
        if not isinstance(dependency, dict) or dependency.get("optional") is not True:
            errors.append(f"{package} must be an optional direct dependency")
        if f"dep:{package}" not in product_domains:
            errors.append(f"{package} must be activated by product-domains")
    if "product-domains" not in features.get("product", []):
        errors.append("product must activate product-domains")
    if "product" not in features.get("task-capsule", []):
        errors.append("task-capsule must preserve product-domains through product")
    if features.get("kernel-host") != []:
        errors.append("kernel-host must not activate any additional feature or dependency")

    roots = declared_roots()
    activations = {
        item
        for feature in (
            "product",
            "product-domains",
            "tui",
            "self-update",
            "local-embeddings",
        )
        for item in features.get(feature, [])
        if item.startswith("dep:")
    }
    for package, dependency in sorted(dependencies.items()):
        if package in roots:
            if isinstance(dependency, dict) and dependency.get("optional") is True:
                errors.append(f"kernel direct root must be unconditional: {package}")
        elif not isinstance(dependency, dict) or dependency.get("optional") is not True:
            errors.append(f"non-kernel direct dependency must be optional: {package}")
        elif f"dep:{package}" not in activations:
            errors.append(f"optional direct dependency has no product feature owner: {package}")
    for package in sorted(roots):
        if package not in dependencies:
            errors.append(f"declared positive root is not a direct dependency: {package}")
    return errors


def main() -> int:
    manifest = tomllib.loads(MANIFEST.read_text())
    errors = manifest_errors(manifest)
    if errors:
        print("kernel-host manifest boundary violated:", file=sys.stderr)
        for error in errors:
            print(f"  {error}", file=sys.stderr)
        return 1

    metadata = subprocess.run(
        [
            "cargo",
            "metadata",
            "--locked",
            "--no-default-features",
            "--features",
            "kernel-host",
            "--format-version",
            "1",
        ],
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if metadata.returncode != 0:
        sys.stderr.write(metadata.stderr)
        return metadata.returncode
    metadata_document = json.loads(metadata.stdout)
    package = next(
        package
        for package in metadata_document["packages"]
        if Path(package["manifest_path"]).resolve() == MANIFEST
    )
    target = next((target for target in package["targets"] if target["name"] == KERNEL_BIN), None)
    if target is None or target["kind"] != ["bin"] or target["required-features"] != ["kernel-host"]:
        print("cargo metadata did not select the declared kernel binary contract", file=sys.stderr)
        return 1

    tree = subprocess.run(
        [
            "cargo",
            "tree",
            "-p",
            "omegon",
            "--locked",
            "--no-default-features",
            "--features",
            "kernel-host",
            "--edges",
            "normal,build",
            "--prefix",
            "none",
        ],
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if tree.returncode != 0:
        sys.stderr.write(tree.stderr)
        return tree.returncode
    forbidden = product_packages(tree.stdout)
    if forbidden:
        print("kernel-host dependency boundary violated: " + ", ".join(forbidden))
        return 1
    root_errors = direct_root_errors(declared_roots(), enabled_direct_roots(metadata_document))
    if root_errors:
        print("kernel-host direct dependency boundary violated:", file=sys.stderr)
        for error in root_errors:
            print(f"  {error}", file=sys.stderr)
        return 1
    print(
        f"kernel-host dependency boundary clean for {KERNEL_BIN}; exact direct roots and "
        "forbidden transitive absence verified"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
