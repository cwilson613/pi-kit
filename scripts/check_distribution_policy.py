#!/usr/bin/env python3
"""Validate target-specific distribution composition policy and source evidence."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
POLICY_PATH = ROOT / "fixtures" / "release-composition-matrix-v1.json"
RELEASE_TARGETS = {
    "aarch64-apple-darwin",
    "x86_64-apple-darwin",
    "aarch64-unknown-linux-gnu",
    "x86_64-unknown-linux-gnu",
    "x86_64-unknown-linux-musl",
}
EXPECTED_COVERAGE = {
    "release-archive": RELEASE_TARGETS,
    "direct-installer": RELEASE_TARGETS,
    "homebrew": RELEASE_TARGETS - {"x86_64-unknown-linux-musl"},
    "nix": RELEASE_TARGETS - {"x86_64-unknown-linux-musl"},
    "oci": {"aarch64-unknown-linux-gnu", "x86_64-unknown-linux-gnu"},
}
HOST_PROFILES = {"full-product", "kernel-host-v1"}
COMPOSITION_CLASSES = {"full-product", "kernel-only", "kernel-plus-codescan-v1", "host-only"}
ROW_KEYS = {
    "distribution",
    "target",
    "host_profile",
    "composition_class",
    "core_components",
    "sdk_extensions",
}


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def validate_policy(document: dict[str, Any]) -> None:
    policy = document.get("distribution_policy")
    require(isinstance(policy, dict), "distribution_policy must be an object")
    require(policy.get("schema_version") == 1, "unsupported distribution policy schema")
    require(
        set(policy) == {"schema_version", "core_component_catalog", "rows", "unsupported"},
        "distribution policy has missing or unknown fields",
    )
    catalog = policy["core_component_catalog"]
    require(
        catalog == {"core:codescan": {"wire_manifest_id": "omegon-codescan"}},
        "core component catalog is not canonical",
    )
    rows = policy["rows"]
    require(isinstance(rows, list), "distribution rows must be an array")
    actual_coverage: dict[str, set[str]] = {name: set() for name in EXPECTED_COVERAGE}
    seen: set[tuple[str, str]] = set()
    for row in rows:
        require(isinstance(row, dict), "distribution row must be an object")
        composition = row.get("composition_class")
        expected_keys = ROW_KEYS | ({"non_parity"} if composition == "host-only" else set())
        require(set(row) == expected_keys, "distribution row has missing or unknown fields")
        distribution = row["distribution"]
        target = row["target"]
        require(distribution in EXPECTED_COVERAGE, "unknown distribution")
        require(target in EXPECTED_COVERAGE[distribution], "unknown distribution target")
        require((distribution, target) not in seen, "duplicate distribution target")
        seen.add((distribution, target))
        actual_coverage[distribution].add(target)
        require(row["host_profile"] in HOST_PROFILES, "unknown host profile")
        require(composition in COMPOSITION_CLASSES, "unknown composition class")
        components = row["core_components"]
        require(isinstance(components, list), "core component inventory must be an array")
        require(len(components) == len(set(components)), "duplicate core component")
        require(set(components) <= set(catalog), "unknown core component")
        sdk = row["sdk_extensions"]
        require(
            sdk == {"posture": "operator-managed", "core_self_promotion": "forbidden"},
            "SDK extension posture is not canonical",
        )
        if composition == "full-product":
            require(components == ["core:codescan"], "full product inventory is not exact")
        if composition == "host-only":
            require(components == [], "host-only row advertises a core component")
            absence = row["non_parity"]
            require(
                isinstance(absence, dict)
                and set(absence) == {"kind", "missing_core_components", "capability_differences"}
                and absence["kind"] == "host-only-component-absence"
                and absence["missing_core_components"] == ["core:codescan"],
                "host-only typed absence is incomplete",
            )
            differences = absence["capability_differences"]
            require(isinstance(differences, list) and differences, "host-only capability difference is absent")
            require(
                all(
                    item == {
                        "capability": "service:codescan",
                        "status": "unavailable",
                        "operator_message": item.get("operator_message"),
                    }
                    and isinstance(item.get("operator_message"), str)
                    and item["operator_message"].strip()
                    for item in differences
                ),
                "host-only capability difference is not operator-visible or typed",
            )
    require(actual_coverage == EXPECTED_COVERAGE, "distribution target coverage is not exact")
    require(
        policy["unsupported"].get("npm", {}).get("status") == "unsupported"
        and policy["unsupported"]["npm"].get("retained_scaffolding") is True,
        "retained npm scaffolding must be explicitly unsupported",
    )


def validate_source_evidence(document: dict[str, Any], root: Path) -> None:
    validate_policy(document)
    release = (root / ".github/workflows/release.yml").read_text()
    installer = (root / "core/install.sh").read_text()
    formula = (root / "homebrew/Formula/omegon.rb").read_text()
    flake = (root / "flake.nix").read_text()
    oci = (root / "nix/oci.nix").read_text()
    npm_workflow = root / ".github/workflows/publish.yml"
    for target in RELEASE_TARGETS:
        require(target in release, f"release workflow lacks target {target}")
        require(target in installer, f"direct installer lacks target {target}")
    require("--codescan-binary" in release and "--codescan-manifest" in release, "release archives omit codescan")
    require("release-coupled codescan extension not found" in installer, "installer does not require codescan")
    require('share.install "share/omegon"' in formula, "Homebrew does not install packaged share assets")
    require("omegon-codescan/manifest.toml" in formula, "Homebrew does not verify codescan")
    require('cargoExtraArgs = "-p omegon -p omegon-maintain"' in flake, "Nix host build changed")
    require("flake-utils.lib.eachDefaultSystem" in flake, "Nix default-system coverage changed")
    require(
        "images = pkgs.lib.optionalAttrs pkgs.stdenv.isLinux" in flake,
        "OCI Linux-only target coverage changed",
    )
    require("extensions/omegon-codescan" not in flake, "Nix unexpectedly packages codescan")
    require("The image contains NO extension binaries" in oci, "OCI host-only evidence changed")
    require(not npm_workflow.exists(), "npm publication workflow makes unsupported status stale")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--policy", type=Path, default=POLICY_PATH)
    args = parser.parse_args()
    try:
        policy = json.loads(args.policy.read_text())
        validate_policy(policy)
        validate_source_evidence(policy, ROOT)
    except (OSError, json.JSONDecodeError, ValueError) as error:
        parser.error(str(error))
    print("distribution policy: ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
