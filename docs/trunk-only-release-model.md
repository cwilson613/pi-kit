---
id: trunk-only-release-model
title: "Trunk-only release model — retire the permanent release branch"
status: decided
tags: []
open_questions:
  - "[assumption] `main` is always releasable. Trunk-only requires that any commit on `main` could be tagged stable. Measured support: 4056 tests pass on every change and the cadence is 1–2 days, so unshippable work never sits on trunk long. Unvalidated: whether a long-lived breaking change (e.g. a multi-week 0.30 refactor) would violate this. If it would, that is exactly when a reactive `release/X.Y` gets cut."
  - "[assumption] No external consumer depends on a `release/X.Y` branch ref. Homebrew, the install script, and OCI images consume tags and GitHub Releases, not branch names. Unvalidated: whether any downstream automation, documentation, or user pins `release/0.28` directly. Must be checked before deleting the branch — retiring it is reversible only if nothing already resolved against it."
  - "[assumption] `verify-publish` retargeted at `main` still protects something real. Its current invariant — origin/main must not advertise an older version than the tag — exists specifically because a release branch could publish ahead of trunk. Under trunk-only that comparison becomes tautological (the tag *is* on main). Open: does the gate degrade into a no-op that should be replaced with a different check (e.g. tag ancestry — the tag must be reachable from origin/main), or removed outright?"
  - "Does the 0.28 line still need patches after v0.28.11? Retirement is only clean once the line is closed. If a 0.28.12 is anticipated, `release/0.28` stays until it ships. Decision needed from the operator, not derivable from the repo."
  - "[assumption] Trunk version stays in prerelease form (`X.Y.0-dev`) between releases, and the release cut is what stamps a stable version. This matters because nightly derives `MAJOR_MINOR.0-nightly.DATE` from `Cargo.toml`, and because leaving `main` on a bare stable version is what produced the 0.28.9-on-trunk state that blocked v0.28.10. Unvalidated: what the exact stable-tagging procedure becomes when there is no branch to bump on — specifically whether `main` carries `0.29.0-dev`, gets tagged `v0.29.0` via a stamped release commit like nightly does, then returns to `0.30.0-dev`."
dependencies: []
related: []
---

# Trunk-only release model — retire the permanent release branch

## Overview

Collapse Omegon's dual-line release model (permanent `release/X.Y` stabilization branch + nightly from `main`) into a trunk-only model: stable tags cut directly from `main`, nightly unchanged, and `release/X.Y` retained as a reactive capability rather than a standing obligation.

## Decisions

### Retirement is safe — nothing pins a release branch ref

**Status:** exploring

**Rationale:**

### Replace the version-comparison gate with a tag-ancestry check

**Status:** exploring

**Rationale:**

### Stable tagging procedure: stamp, tag, reopen

**Status:** exploring

**Rationale:**

### 0.28 is closed; release branches become reactive-only

**Status:** exploring

**Rationale:**

## Open Questions

- [assumption] `main` is always releasable. Trunk-only requires that any commit on `main` could be tagged stable. Measured support: 4056 tests pass on every change and the cadence is 1–2 days, so unshippable work never sits on trunk long. Unvalidated: whether a long-lived breaking change (e.g. a multi-week 0.30 refactor) would violate this. If it would, that is exactly when a reactive `release/X.Y` gets cut.
- [assumption] No external consumer depends on a `release/X.Y` branch ref. Homebrew, the install script, and OCI images consume tags and GitHub Releases, not branch names. Unvalidated: whether any downstream automation, documentation, or user pins `release/0.28` directly. Must be checked before deleting the branch — retiring it is reversible only if nothing already resolved against it.
- [assumption] `verify-publish` retargeted at `main` still protects something real. Its current invariant — origin/main must not advertise an older version than the tag — exists specifically because a release branch could publish ahead of trunk. Under trunk-only that comparison becomes tautological (the tag *is* on main). Open: does the gate degrade into a no-op that should be replaced with a different check (e.g. tag ancestry — the tag must be reachable from origin/main), or removed outright?
- Does the 0.28 line still need patches after v0.28.11? Retirement is only clean once the line is closed. If a 0.28.12 is anticipated, `release/0.28` stays until it ships. Decision needed from the operator, not derivable from the repo.
- [assumption] Trunk version stays in prerelease form (`X.Y.0-dev`) between releases, and the release cut is what stamps a stable version. This matters because nightly derives `MAJOR_MINOR.0-nightly.DATE` from `Cargo.toml`, and because leaving `main` on a bare stable version is what produced the 0.28.9-on-trunk state that blocked v0.28.10. Unvalidated: what the exact stable-tagging procedure becomes when there is no branch to bump on — specifically whether `main` carries `0.29.0-dev`, gets tagged `v0.29.0` via a stamped release commit like nightly does, then returns to `0.30.0-dev`.
