+++
id = "0e5685f9-3cae-4575-80ef-adcb3f98426b"
kind = "document"
title = "Omegon Installation & Distribution"
status = "implemented"
tags = ["distribution", "dx", "packaging"]
aliases = ["omegon-install"]
imported_reference = false

[publication]
enabled = false
visibility = "private"

[data]
open_questions = []
+++

# Omegon Installation & Distribution

## Overview

Engineers should be able to install Omegon with a single command: no git clone, no submodule init, no npm runtime, and no manual link step. Release archives, the direct installer, and Homebrew install the full product. That product contains `omegon`, `omegon-maintain`, bundled content, and signed product component `core:codescan`.

The full-product layout stores codescan composition evidence at
`share/omegon/components/core-codescan.lock.json`. The lock binds the component
and wire identities to the packaged manifest and executable digests, target,
protocol, fallback policy, and release workflow. Package verification,
installation, update staging, and runtime startup use this record. Do not use a
host composition lock or an SDK extension manifest as product ownership
evidence.

Current install surfaces:

- install script: `curl -fsSL https://omegon.styrene.io/install.sh | sh`
- nightlies: `curl -fsSL https://omegon.styrene.io/install.sh | sh -s -- --channel=nightly`
- Homebrew: `brew tap styrene-lab/tap && brew install omegon`
- direct GitHub release artifacts from `styrene-lab/omegon`

Nix packages and Nix-built OCI images currently install the default compiled host without product-component binaries. They are host-only distributions, not full-product equivalents. Codescan reports typed `service:codescan` unavailability until the operator installs and manages a compatible SDK extension. OCI images use the documented init-container and shared-volume pattern for such extensions.

The machine-readable target matrix is `fixtures/release-composition-matrix-v1.json`. It is authoritative for host profile, installation composition class, signed `core:*` inventory, and SDK-extension posture. Files under `core/npm/` are retained legacy scaffolding. npm is not a supported install or publication channel.

Release validation executes the packaged layouts rather than relying on archive
names or formula syntax. Native release jobs install a local archive through the
direct installer with networking disabled, and the Homebrew update job applies
the formula destinations before running the same full-product codescan
acceptance. Pull requests execute the packaged Nix host on Linux. Stable OCI
jobs load each image locally and require `full-product` host-profile and
`host-only` composition labels, typed `service:unavailable` for `core:codescan`,
and zero extension processes before pushing the image.

Source checkouts use `just link`, which performs its own release build for both executables. It installs stable development launchers into `~/.local/bin/omegon`, `~/.local/bin/om`, and `~/.local/bin/omegon-maintain`, registers the checkout in `~/.omegon/channels/default`, and keeps fallback copies in `~/.omegon/bin/`. It does not use shell-profile aliases as the primary resolution mechanism. Run `omegon --which` and `omegon-maintain --which` to inspect the resolved targets; both must report the current checkout commit and `stale: no`.

## Linux runtime requirements

**Important:** Homebrew on Linux does **not** solve host glibc ABI compatibility for Omegon release binaries.

If a Linux release artifact was built against a newer glibc than your distro provides, install may succeed but the binary will fail immediately at runtime with errors like:

```text
omegon: /lib/x86_64-linux-gnu/libc.so.6: version `GLIBC_2.38' not found
omegon: /lib/x86_64-linux-gnu/libc.so.6: version `GLIBC_2.39' not found
```

That means the host system glibc is older than the binary expects.

### Current expectation

Before relying on a Linux Homebrew install, verify that your host glibc is new enough for the shipped release artifact:

```bash
ldd --version
```

If your system glibc is older than the required version for the current release artifact, `brew install` alone is not sufficient.

### What to do if this happens

Use one of these paths:

- run Omegon on a newer Linux distribution with a compatible glibc
- use a container/VM image that provides the required glibc baseline
- use another distribution channel once an older-glibc or musl/static Linux artifact is published

### Documentation contract

Linux install surfaces must state runtime ABI requirements explicitly. `brew install` should never imply that Homebrew will supply a compatible glibc for Omegon binaries on Linux.

## Update contract

Omegon is one release-coupled installed product boundary. There is no Node.js runtime package or companion TypeScript fork to update separately, but the independent Rust maintenance executable must match the normal runtime release.

The authoritative update path therefore must:
- mutate the installed runtime surface (`/update install`, `brew upgrade omegon`, or reinstall via `install.sh` depending on channel)
- replace and validate `omegon` plus `omegon-maintain` together
- verify the active `omegon` / `om` and `omegon-maintain` launchers resolve to the matching release
- stop at a deliberate restart handoff that tells the operator to relaunch `om` or `omegon`

`/refresh` is intentionally narrower: it only clears transient caches and reloads extensions. It is not equivalent to `/update` after package/runtime mutation.

Script-managed installs use the `versioned-current-v1` layout. Each immutable `~/.omegon/versions/<version>/` generation contains `omegon`, `omegon-maintain`, both resident composition locks, bundled content, generation-local `core:codescan` manifest, executable, and component lock, plus that generation's `install-receipt.json`. The stable `omegon`, `om`, `omegon-maintain`, and `~/.config/omegon/install-receipt.json` paths resolve through the single `~/.omegon/current` symlink. Install, self-update, rollback, and version switching validate this complete digest-bound generation before atomically replacing `current`; staging, extraction, or verification failure leaves the previous host, component, and receipt selected and callable.

Release lifecycle operations never install or clean up `core:codescan` through `~/.omegon/extensions/omegon-codescan`. That path is operator-managed SDK-extension state. A same-named operator extension is preserved across install, update, switch, rollback, and failed-generation cleanup. `just link` also refuses to replace an operator-managed collision unless the operator removes it explicitly; its marked development install is separate from release generations.

## Decisions

### Decision: Rust executable pair is the product boundary

**Status:** implemented
**Rationale:** The installable product is the release-coupled normal runtime and independent recovery companion plus bundled assets. This avoids Node/npm runtime dependency drift and submodule packaging failures while preserving recovery when normal startup inputs are broken.

### Decision: Release artifacts are CI-owned

**Status:** implemented
**Rationale:** Operator workstations may build and sign local validation binaries, but distributable archives, checksums, signatures, attestations, Homebrew updates, and site deployments should come from CI.

### Decision: Repo under styrene-lab GitHub org

**Status:** decided
**Rationale:** The canonical upstream is `styrene-lab/omegon`. Install, update, release, and docs links should use that owner.

## Open Questions

*No open questions.*

## Implementation Notes

### File Scope

- `site/src/pages/docs/install.astro` — public install docs
- `site/src/pages/docs/recovery.astro` — public maintenance and recovery workflows
- `site/snippets/install.yaml` — canonical install commands
- `site/snippets/maintenance.yaml` and `site/snippets/verify.yaml` — canonical recovery and offline verification commands
- `Justfile` — source build, validation, and local link recipes
- `.github/workflows/*` — CI release/site artifact production
- `homebrew/` — Homebrew packaging metadata

### Constraints

- Linux artifacts must state their glibc baseline clearly.
- Homebrew-managed installs should update through Homebrew.
- Script-managed installs should update by rerunning the install script or using `/update`.
- Source checkout development should use `just link`; do not run a redundant release build first.
- Missing or mismatched companions fail package and update validation; never repair a pair by copying one executable from another release.
- Script-managed release activation changes only `~/.omegon/current`; never repoint the public executable or receipt links independently to select a version.
- Release generation cleanup is confined to staging directories and immutable members under `~/.omegon/versions`; it must not remove operator-managed SDK extensions under `~/.omegon/extensions`.

## Migration note

Older TypeScript/npm/pi distribution notes are historical only. Retained npm package files are not published and do not define current target support. New docs, scripts, and release automation should describe the release-coupled Rust product.
