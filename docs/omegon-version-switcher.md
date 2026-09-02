+++
id = "e7151d51-69bc-42e3-85ce-eb1674d21368"
kind = "document"
title = "Version switcher — tfswitch-style binary management for Omegon"
status = "implemented"
tags = []
aliases = ["omegon-version-switcher"]
imported_reference = false

[publication]
enabled = false
visibility = "private"

[data]
branches = ["feature/omegon-version-switcher"]
open_questions = []
openspec_change = "omegon-version-switcher"
parent = "release-candidate-system"
+++

# Version switcher — tfswitch-style binary management for Omegon

## Overview

> Parent: [Release candidate system — identifiable pre-release builds with deployment verification](release-candidate-system.md)
> Spawned from: "How should RC builds be distributed to other machines? Options: GitHub release (pre-release tag), install.sh with channel flag (--rc), scp/direct copy, or cargo install from git ref"

The implemented switcher manages the release-coupled `omegon` and `omegon-maintain` pair as immutable installed generations. A shared activation symlink selects the pair and matching install receipt together, so a crash cannot expose executables from different releases.

## Research

### tfswitch design reference

tfswitch (github.com/warrensbox/terraform-switcher) is a Go CLI that manages multiple Terraform binary versions:

**Storage**: `~/.terraform.versions/terraform_0.14.0` — flat directory, one binary per version.

**Switching**: Copies (or symlinks) the selected binary to a PATH location. The active version is whatever binary is at the symlink target.

**Download**: Fetches from HashiCorp's releases page on demand. Checksums verified. Cached — only downloads once per version.

**Auto-detection**: Reads `.terraform-version` (single version string) or `.tfswitchrc` from the project root. Also reads `required_version` from Terraform module files. Running `tfswitch` with no args in a project auto-selects.

**CLI**: `tfswitch` (interactive picker), `tfswitch 1.5.0` (exact), `tfswitch --latest-stable 1.5` (latest matching prefix), `tfswitch --latest-pre 0.13` (latest pre-release matching prefix).

**Key insight**: tfswitch doesn't build anything. It downloads pre-built binaries from releases. The version manager is separate from the build system.

### Omegon version switcher design

**Storage**: `~/.omegon/versions/0.14.1/` — immutable complete release generation containing the executable pair, bundled content, signed components, composition locks, and `install-receipt.json`.

**Active version**: `~/.omegon/current` is one atomic symlink to a complete version directory. Stable launchers for `omegon`, `om`, and `omegon-maintain`, plus `~/.config/omegon/install-receipt.json`, resolve through `current`. Changing one link therefore changes the executable pair and receipt together.

**Download source**: GitHub Releases from `styrene-lab/omegon`. The switcher captures `omegon-maintain` from the active generation and uses it to authenticate the archive, signed package manifest, and Sigstore bundle before and after extraction. Checksums provide integrity evidence but are not the release authority. The switcher writes the derived receipt and publishes the complete generation before activation. Stable and nightly releases use the same generation contract.

**CLI surface**:
- `omegon switch` — interactive TUI picker showing installed + available versions
- `omegon switch 0.14.1` — install (if needed) and switch to an exact version
- `omegon switch --latest` — switch to latest stable release
- `omegon switch --list` — show installed versions, highlight active

**Auto-detection**: `.omegon-version` file in project root. Contains a version string or constraint. When `omegon` starts, if `.omegon-version` exists and the requested version isn't active, it either auto-switches or warns.

**Self-update**: `/update install`, direct installation, and `omegon switch` share the `versioned-current-v1` activation contract. The switcher captures `omegon-maintain` from the active generation and uses only that executable to authenticate the canonical signed archive before and after extraction. Candidate publication cannot change the running release. After the complete generation validates, one atomic replacement of `current` selects the host, maintenance companion, components, content, locks, and receipt; the new binary takes over on the next invocation.

**Switcher subcommand with independent recovery companion**: Unlike tfswitch, version selection is an `omegon switch` subcommand rather than a separate version-manager program. `omegon-maintain` remains a distinct required recovery executable and is always installed and switched with `omegon`.

**Interruption and rollback**: Download, extraction, receipt creation, and validation happen before activation. Interruption before the activation rename leaves the previous generation active. Published generations are immutable and retained, so switching back selects the prior complete pair rather than reconstructing one in place.

## Decisions

### Decision: Subcommand and shared generation activation

**Status:** implemented
**Rationale:** Keeping selection in `omegon switch` avoids another version-manager artifact, while immutable complete generations preserve the independently runnable maintenance boundary. A single shared activation link is the smallest crash-atomic selection primitive: no observer can resolve a new `omegon` with an old maintenance companion or receipt. Dev machines keep `just link` for source builds; the switcher is for installer-managed machines consuming GitHub Release artifacts.

## Open Questions

*No open questions.*

## Implementation Notes

### File Scope

- `core/crates/omegon/src/installed_release.rs` — shared immutable-generation publication and atomic activation
- `core/crates/omegon/src/switch.rs` — list releases, download and verify complete pairs, publish generations, and select versions
- `core/crates/omegon/src/update.rs` — migrate managed installs and self-update through the shared layout
- `core/install.sh` — create and migrate `versioned-current-v1` direct installs

### Constraints

- Downloads from the `styrene-lab/omegon` GitHub Releases API
- Platform detection: aarch64-apple-darwin, x86_64-apple-darwin, x86_64-unknown-linux-gnu, aarch64-unknown-linux-gnu
- The active generation's `omegon-maintain` authenticates the archive, package manifest, and Sigstore bundle. SHA-256 checksums are supplementary integrity evidence.
- Every installed generation contains matching executables, bundled content, signed components, composition locks, and `install-receipt.json`.
- Version selection atomically replaces `~/.omegon/current`; stable launchers are not independently version-selecting links.
- Interactive picker: simple terminal list with arrow keys, no ratatui dependency (runs outside TUI)
- .omegon-version auto-detect: read file from cwd ancestors, warn if active version doesn't match
