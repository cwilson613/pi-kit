+++
id = "headless-release-artifacts"
tags = ["release", "headless", "packaging", "feature-flags"]
aliases = ["headless-distribution"]
imported_reference = false

[publication]
enabled = false
visibility = "private"
+++

# Headless release artifacts

## Status

**Exploring.** This node records the assessment for publishing no-TUI Omegon binaries as a first-class release flavor. No implementation is authorized by this node.

## Overview

Omegon now compiles with `--no-default-features`, and the repository enforces a dependency boundary that excludes terminal/TUI crates from that graph. Evaluate publishing Linux headless artifacts for server, CI, container, and remote-daemon deployments without replacing the default TUI distribution.

## Current evidence

- `core/crates/omegon/Cargo.toml` defines `default = ["tui"]`; `--no-default-features` produces the headless configuration.
- `scripts/check_headless_dependency_boundary.py` rejects Ratatui, Crossterm, TachyonFX, image, and related TUI dependencies in the headless graph.
- The no-TUI interactive path directs operators to `omegon serve`.
- `.github/workflows/release.yml` currently builds and publishes one default artifact per target.
- `scripts/release_manifest.py` currently models assets by target only, so it cannot safely distinguish default and headless update channels.

## Proposed direction

Publish a distinct archive flavor while retaining `omegon` as the executable name:

```text
omegon-headless-<version>-<target>.tar.gz
```

Initial target set:

- `x86_64-unknown-linux-gnu`
- `x86_64-unknown-linux-musl`
- `aarch64-unknown-linux-gnu`

The default distribution and Homebrew formula remain unchanged.

## Decisions under consideration

1. **Distribution flavor is explicit metadata.** Release manifests and updater selection must key on both target and flavor. Older manifests without a flavor are interpreted as `default`.
2. **Self-update preserves flavor.** A headless binary must never silently replace itself with the default/TUI artifact.
3. **One source package and executable.** Do not create an `omegon-headless` crate or rename the binary; Cargo features remain the build boundary.
4. **Linux-first rollout.** Do not add macOS headless signing/notarization cost until demand is demonstrated.
5. **Headless artifacts are release-blocking once advertised.** Missing supported headless targets should fail stable publication rather than produce a partial channel.

## Open questions

- [assumption] Server and container operators need downloadable headless archives in addition to existing OCI images.
- [assumption] `default` and `headless` are sufficient flavor identifiers even if future feature bundles are added.
- Should headless artifacts be published for nightly releases from the first iteration?
- Should installer selection use `--flavor headless`, a dedicated installer endpoint, or both?
- Can the existing CycloneDX tooling produce feature-accurate artifact SBOMs, or is a documented limitation required?
- Should `aarch64-unknown-linux-musl` be added before or after the initial three-target rollout?

## Required implementation surfaces

- `.github/workflows/release.yml` — Linux headless build matrix, smoke tests, archives, checksums, signing, attestation, and ABI/static-link validation.
- `scripts/release_manifest.py` — flavor-aware parsing and schema; preserve default-only Homebrew generation.
- `core/crates/omegon/src/update.rs` — compile-time flavor identity and target-plus-flavor artifact selection.
- `install.sh` — explicit opt-in headless installation without changing default behavior.
- `scripts/release_preflight.py` and tests — completeness and workflow wiring checks.
- CI — required no-default-features compile/test and dependency-boundary gates.
- Release documentation — supported commands and daemon/service deployment contract.

## Acceptance criteria for a future implementation

- Default artifacts and Homebrew behavior are unchanged.
- Headless archives contain an executable named `omegon`.
- A released headless binary passes `--version`, `--help`, and `serve --help`, starts a daemon on an ephemeral port, and shuts down cleanly.
- Linux ABI checks cover both default and headless GNU artifacts; MUSL headless artifacts are verified static.
- Release metadata rejects duplicate `(target, flavor)` entries.
- Updater tests prove that headless installations cannot cross-grade to default artifacts.
- Every published archive has checksums, signatures, certificates, and provenance attestations.

## Tradeoffs

Benefits are a smaller dependency graph, reduced server attack surface, and a distribution aligned with noninteractive deployments. Costs are a second supported release configuration, more CI minutes and artifacts, flavor-aware updater/installer complexity, and the risk of feature drift. The existing dependency-boundary script reduces but does not eliminate that risk.
