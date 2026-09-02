#!/usr/bin/env python3
"""Validate the operational kernel/core/addon acceptance corpus."""

from __future__ import annotations

import argparse
import json
import re
import subprocess
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
CORPUS_PATH = ROOT / "fixtures" / "operational-kernel-core-corpus-v1.json"
AXES = {
    "artifact",
    "execution",
    "authority",
    "surface",
    "lifecycle",
    "generation",
    "fault",
    "cleanup",
    "distribution",
}
EVIDENCE_STATES = {"implemented", "planned"}
SCENARIO_ID = re.compile(r"^(?P<family>[A-Z]{3})-[0-9]{3}$")
REQUIRED_EXECUTOR_MARKERS = {
    "LIF-001": ("lif_001",),
    "LIF-003": ("lif_003",),
    "SUR-001": ("surface_parity_campaign", "sur_001"),
    "SUR-002": ("surface_parity_campaign", "sur_002", "delayed_prior_turn_terminal_advice"),
}


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def load_corpus(path: Path = CORPUS_PATH) -> dict[str, Any]:
    return json.loads(path.read_text())


def validate_corpus(document: dict[str, Any], root: Path = ROOT) -> None:
    require(
        set(document)
        == {
            "schema_version",
            "dimensions",
            "invariants",
            "families",
            "scenarios",
            "promotion_profiles",
        },
        "corpus has missing or unknown top-level fields",
    )
    require(document["schema_version"] == 1, "unsupported corpus schema")
    dimensions = document["dimensions"]
    require(set(dimensions) == AXES, "corpus dimensions are not exact")
    for name, values in dimensions.items():
        require(isinstance(values, list) and values, f"dimension {name} is empty")
        require(len(values) == len(set(values)), f"dimension {name} contains duplicates")

    invariants = document["invariants"]
    require(isinstance(invariants, list) and invariants, "invariant catalog is empty")
    require(len(invariants) == len(set(invariants)), "invariant catalog contains duplicates")
    invariant_set = set(invariants)
    families = document["families"]
    require(isinstance(families, dict) and families, "scenario family catalog is empty")

    scenarios = document["scenarios"]
    require(isinstance(scenarios, list) and scenarios, "scenario catalog is empty")
    seen: set[str] = set()
    scenario_by_id: dict[str, dict[str, Any]] = {}
    for scenario in scenarios:
        require(
            set(scenario)
            == {"id", "title", "requirement", "axes", "invariants", "oracle", "evidence"},
            "scenario has missing or unknown fields",
        )
        scenario_id = scenario["id"]
        match = SCENARIO_ID.fullmatch(scenario_id)
        require(match is not None, f"invalid scenario id: {scenario_id}")
        require(scenario_id not in seen, f"duplicate scenario id: {scenario_id}")
        seen.add(scenario_id)
        scenario_by_id[scenario_id] = scenario
        require(match.group("family") in families, f"unknown scenario family: {scenario_id}")
        require(isinstance(scenario["title"], str) and scenario["title"].strip(), f"{scenario_id}: title is empty")
        require(
            isinstance(scenario["requirement"], str) and ":" in scenario["requirement"],
            f"{scenario_id}: requirement reference is invalid",
        )

        axes = scenario["axes"]
        require(set(axes) == AXES, f"{scenario_id}: axes are not exact")
        for name, value in axes.items():
            require(value in dimensions[name], f"{scenario_id}: unknown {name} value {value}")
        selected_invariants = scenario["invariants"]
        require(
            isinstance(selected_invariants, list) and selected_invariants,
            f"{scenario_id}: invariants are empty",
        )
        require(
            set(selected_invariants) <= invariant_set,
            f"{scenario_id}: unknown invariant",
        )
        require(
            isinstance(scenario["oracle"], list)
            and scenario["oracle"]
            and all(isinstance(item, str) and item.strip() for item in scenario["oracle"]),
            f"{scenario_id}: oracle is empty or invalid",
        )

        evidence = scenario["evidence"]
        require(set(evidence) == {"status", "executors"}, f"{scenario_id}: evidence fields are not exact")
        require(evidence["status"] in EVIDENCE_STATES, f"{scenario_id}: invalid evidence status")
        executors = evidence["executors"]
        require(isinstance(executors, list), f"{scenario_id}: executors must be an array")
        if evidence["status"] == "implemented":
            require(executors, f"{scenario_id}: implemented evidence has no executor")
        for executor in executors:
            require(set(executor) == {"path", "command"}, f"{scenario_id}: executor fields are not exact")
            path = root / executor["path"]
            require(path.exists(), f"{scenario_id}: executor path does not exist: {executor['path']}")
            require(
                isinstance(executor["command"], str) and executor["command"].strip(),
                f"{scenario_id}: executor command is empty",
            )
        if evidence["status"] == "implemented" and scenario_id in REQUIRED_EXECUTOR_MARKERS:
            bound_evidence = "\n".join(
                f"{executor['path']} {executor['command']}" for executor in executors
            )
            for marker in REQUIRED_EXECUTOR_MARKERS[scenario_id]:
                require(
                    marker in bound_evidence,
                    f"{scenario_id}: executors do not bind required evidence marker {marker}",
                )

    profiles = document["promotion_profiles"]
    require(isinstance(profiles, dict) and profiles, "promotion profiles are empty")
    for profile, scenario_ids in profiles.items():
        require(isinstance(scenario_ids, list) and scenario_ids, f"profile {profile} is empty")
        require(len(scenario_ids) == len(set(scenario_ids)), f"profile {profile} contains duplicates")
        require(set(scenario_ids) <= seen, f"profile {profile} references an unknown scenario")
    require(
        set(profiles["milestone-pr-readiness"]) == seen,
        "milestone PR-readiness profile must contain every scenario",
    )


def incomplete_profile(document: dict[str, Any], profile: str) -> list[str]:
    profiles = document["promotion_profiles"]
    require(profile in profiles, f"unknown promotion profile: {profile}")
    scenarios = {scenario["id"]: scenario for scenario in document["scenarios"]}
    return [
        scenario_id
        for scenario_id in profiles[profile]
        if scenarios[scenario_id]["evidence"]["status"] != "implemented"
    ]


def profile_commands(document: dict[str, Any], profiles: list[str]) -> list[str]:
    scenarios = {scenario["id"]: scenario for scenario in document["scenarios"]}
    commands: list[str] = []
    seen: set[str] = set()
    for profile in profiles:
        require(profile in document["promotion_profiles"], f"unknown promotion profile: {profile}")
        for scenario_id in document["promotion_profiles"][profile]:
            for executor in scenarios[scenario_id]["evidence"]["executors"]:
                command = executor["command"]
                if command not in seen:
                    seen.add(command)
                    commands.append(command)
    return commands


def execute_profiles(document: dict[str, Any], profiles: list[str]) -> None:
    for command in profile_commands(document, profiles):
        print(f"\n==> {command}", flush=True)
        subprocess.run(
            command,
            cwd=ROOT,
            shell=True,
            executable="/bin/bash",
            check=True,
        )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--profile", help="fail if this promotion profile has planned evidence")
    parser.add_argument(
        "--execute-profile",
        action="append",
        default=[],
        help="execute this profile's declared evidence; may be repeated",
    )
    args = parser.parse_args()
    document = load_corpus()
    validate_corpus(document)
    selected_profiles = ([args.profile] if args.profile else []) + args.execute_profile
    for profile in selected_profiles:
        incomplete = incomplete_profile(document, profile)
        if incomplete:
            raise ValueError(
                f"promotion profile {profile} has planned evidence: {', '.join(incomplete)}"
            )
    if args.execute_profile:
        execute_profiles(document, args.execute_profile)
    print(
        f"operational kernel/core corpus valid: scenarios={len(document['scenarios'])}"
        + (f" profiles={','.join(selected_profiles)}" if selected_profiles else "")
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
