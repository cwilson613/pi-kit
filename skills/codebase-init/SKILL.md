+++
id = "2ccdb4d6-3cc7-4be5-88ac-671098393d91"
name = "codebase-init"
description = "Evidence-led initialization and architectural orientation for unfamiliar repositories"
tags = ["analysis", "onboarding"]
aliases = ["repo-init", "codebase-orientation"]
triggers = ["initialize this codebase", "analyze this repository", "orient me in this repo", "assess agents.md", "open a new repo"]
activation = "project_detected"
profile = ["coding"]
project_signals = [".git"]
+++

# Codebase Initialization

Use this skill when opening an unfamiliar repository, refreshing stale repository guidance, or preparing a codebase for reliable agent work. The goal is a compact, evidence-backed operating model—not an exhaustive inventory and not immediate mutation.

## Contract

Initialization has two phases:

1. **Assess** — read-only discovery that produces findings and an explicit proposed plan.
2. **Apply** — operator-approved, minimal changes to durable guidance or harness substrate.

Do not silently create `AGENTS.md`, lifecycle artifacts, memory stores, or configuration during assessment. Omegon's `/init` menu is the explicit mutation surface for harness setup and migrations.

## Phase 1: Establish the boundary

1. Resolve the repository root and current working directory. Account for worktrees, submodules, and nested repositories.
2. Inspect version-control state before changing anything: branch, dirty paths, remotes, and ignored local-runtime directories.
3. Read the nearest applicable instruction files before source code:
   - root and nested `AGENTS.md`;
   - `CONTRIBUTING.md`, `README*`, and architecture/development docs;
   - build manifests, task runners, CI definitions, and release policy;
   - lifecycle/spec artifacts only when the repository actually uses them.
4. Distinguish repository policy from harness defaults and maintainer-only workflow from community contribution requirements.

Stop and report a boundary conflict if nested directives contradict root policy or if the apparent repository root is ambiguous.

## Phase 2: Build an evidence map

Prefer authoritative manifests and executable definitions over prose summaries.

Capture:

- language, package, workspace, and generated-code boundaries;
- component ownership and dependency direction;
- entry points, public interfaces, persistence/wire contracts, and frontend adapters;
- canonical build, format, lint, focused-test, full-test, install, and runtime-exercise commands;
- CI/release gates and changelog expectations;
- security-sensitive boundaries: external input, paths, processes, credentials, network calls, and serialization;
- existing agent conventions, skills, project memory, design records, and OpenSpec/lifecycle state;
- platform assumptions, especially Unix, macOS, Linux, WSL, and native-Windows distinctions.

Use search to locate owners, then read the owning files. Do not infer architecture from filenames alone. Cite paths and line ranges for material claims.

## Phase 3: Assess durable guidance

Evaluate root guidance for:

- stale crate/module inventories;
- commands that no longer exist or duplicate work;
- unconditional expensive gates where scoped validation exists;
- obsolete names, modes, paths, providers, or support claims;
- missing process cleanup, provenance, compatibility, or security invariants;
- volatile counts and line totals presented as durable facts;
- internal workflow accidentally imposed on external contributors.

Evaluate nested `AGENTS.md` placement strategically. Add one only where a subtree has durable local information that materially improves deep analysis:

- a clear ownership boundary;
- compatibility or persistence invariants;
- a specialized validation matrix;
- security or process-lifecycle constraints;
- a high risk of placing behavior in the wrong layer.

Do **not** create one per directory mechanically. Nested guidance augments root policy; it should not duplicate repository-wide rules. Prefer a small first tier at architectural choke points, then expand only from demonstrated analysis failures.

## Phase 4: Produce the initialization report

Before mutation, report:

1. **Current operating model** — concise architecture and workflow.
2. **Verified commands** — focused versus broad gates, with evidence.
3. **Directive drift** — ranked by consequence, not cosmetic age.
4. **Guidance topology** — which root/nested files should exist and why.
5. **Unknowns and assumptions** — explicitly labeled.
6. **Proposed edits** — smallest coherent changes, expected tests, and files affected.

Stop exploring when the next reversible step is justified. Do not turn initialization into open-ended archaeology.

## Phase 5: Apply approved changes

When the operator approves:

1. Make surgical edits; preserve still-valid guidance.
2. Add nested directives only at the selected ownership boundaries.
3. Keep facts sourced and avoid hard-coded volatile inventories where an authoritative command exists.
4. Add or update tests when `/init`, skill activation, parsing, or generated behavior changes.
5. Update release memory when repository policy requires it.
6. Run Markdown/manifest validation, focused behavioral tests, and the appropriate landing gate.
7. Reconcile plan/lifecycle state and leave the worktree clean or explicitly describe remaining changes.

## `/init` integration

When this bundled skill is available, `/init` should surface it as the repository-analysis companion to harness bootstrap:

- **Codebase initialization skill** answers “what is this repository and how should agents work here?”
- **`/init scan`** performs explicit, non-destructive harness setup such as creating configured directories or importing discovered directives.
- **`/init migrate`** performs separately confirmed legacy-layout moves.

Inspection does not imply mutation. Harness setup does not imply that generated guidance is accurate without evidence-led review.
