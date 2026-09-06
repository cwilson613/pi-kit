+++
id = "407c3c50-9350-4476-8084-fdd8f3f639da"
tags = []
aliases = []
imported_reference = false

[publication]
enabled = false
visibility = "private"
+++

# Omegon Project Directives

## What this is

Omegon is a Rust-native agent loop and lifecycle engine. You are working on the tool itself — the codebase you're editing is the same tool that's running you. Be precise.

## Architecture

- **Workspace root**: Cargo workspace at the repository root. Crates live under `core/crates/`:
  - `omegon` — main binary, agent loop, providers, tools, TUI, ACP, daemon/control plane, and integration composition
  - `omegon-codescan-contracts` — portable codescan request, response, and status types without indexing dependencies
  - `omegon-codescan` — code and knowledge indexing
  - `omegon-git` — repository, commit, merge, worktree, and submodule operations
  - `omegon-memory` — fact storage, decay, search, injection, and vault synchronization
  - `omegon-opsx` — OpenSpec/design lifecycle state machine
  - `omegon-rbac` — Omegon capability vocabulary mapped onto Styrene RBAC
  - `omegon-secrets` — secret resolution, redaction, and tool guards
  - `omegon-skills` — skill parsing, inventory, activation, and suggestion policy
  - `omegon-traits` — shared protocol, feature, command, tool, and event contracts
  - `omegon-web` — web search and content extraction (not the embedded dashboard, which remains in `omegon`)
  - `styrene-work-model` — provider-neutral work-item contracts
  - `styrene-work-runtime` — work-source refresh and immutable aggregate snapshots
- **First-party extensions**: `extensions/omegon-codescan` owns the release-coupled codescan process, SQLite connection, indexing worker, and RPC lifecycle. The `omegon` binary must not depend on the `omegon-codescan` engine crate.
- **Build and run**: `just run` rebuilds and launches the current `dev-release` binary. `just link` performs its own release build, installs the stable launchers, registers the checkout/channel, and installs bundled skills/catalog plus the codescan native extension; do not precede it with a redundant `just build` unless a standalone release build is itself required.
- **Validation ladder**: use the narrowest relevant test while iterating. For an isolated single-crate change, land with `just test-crate <crate>` (or its feature-specific recipe) plus `just clippy-changed`; `just test-secrets` covers both shipped `omegon-secrets` configurations. Reserve `just test-commit` for multi-crate changes, shared contracts/dependencies, or cases where reverse-dependent coverage is materially useful—it may cold-build every affected crate. Use `just lint` and serialized `just test-rust` for broad/high-risk changes and release hardening.
- **Long-running Cargo gates**: cold dependency/feature builds are routinely longer than blocking tool-call ceilings. A timeout without compiler/test failure is indeterminate, not a failed gate. Start long gates in an interactive terminal and monitor them to completion; do not repeatedly restart a cold build or halt progress because a short wrapper timeout expired.
- **Single crate**: `just test-crate omegon-memory`
- **Secrets configurations**: `just test-secrets`
- **Filter**: `just test-filter "vault_sync"`
- **Config schemas**: `pkl/` contains the Pkl schemas for configuration surfaces. Avoid embedding a schema count here; it changes over time.
- **Skills**: `skills/*/SKILL.md` — YAML frontmatter is canonical for portable skills; TOML frontmatter remains supported for bundled/existing skills. `name` and `description` are required.
- **Nested directives**: strategic crates may contain their own `AGENTS.md`. The nearest file adds crate-local ownership and invariant guidance; this root file remains authoritative for repository-wide workflow.

## Key conventions

- **Conventional commits** — `feat:`, `fix:`, `refactor:`, `chore:`, `docs:`. See `skills/git/SKILL.md`.
- **Branch and release policy** — `CONTRIBUTING.md` is canonical for human/repo governance. Agents must not start implementation work directly on `main`; create a focused branch before editing code or docs. Do not mutate workspace/package versions, `.omegon/milestones.json`, versioned `CHANGELOG.md` release sections, or release-generated packaging metadata except on branches allowed by `CONTRIBUTING.md`; ordinary dependency manifest and lockfile updates remain valid on focused dependency-update branches.
- **Read before editing** — `edit` requires an exact current-text match. Read the target first and make the smallest justified replacement.
- **Test after changes** — run the narrowest relevant test while iterating, then the landing gate appropriate to the change. Do not claim a full gate passed unless it actually ran.
- **Cargo test filters** — Cargo accepts only one positional test-name filter per invocation. Use one shared substring, separate invocations, or a loop such as `for f in test_one test_two; do cargo test -p omegon "$f" --locked || exit 1; done`.
- **Install only when runtime evidence needs it** — use `just run` to exercise current source directly. Use `just link` when the installed launcher/assets must match the commit, then verify resolution with `omegon --which` when binary identity matters.
- **Happy development loop** — inspect → design → implement → focused test → commit → rebuild/install → exercise the real harness → reconcile Workbench/OpenSpec/design/git state → hand off cleanly.
- **CHANGELOG.md is release memory** — update `[Unreleased]` for behavior, public docs/site output, tooling, packaging, public API, or operator/contributor workflow changes. Trivial typo-only and internal-test-only changes need no entry. Every release/tag needs a complete section for that exact version without skipped released versions.

## Provider system

- Providers are in `providers.rs`. Each has a client struct implementing `LlmBridge`.
- Tool schemas are normalized per-provider via `tool_schema.rs` (Full/OpenAI/Gemini dialects).
- OAuth credentials: Anthropic and OpenAI client IDs are public (shipped by their CLIs). Google Gemini CLI credentials are public per Google's installed-app policy.
- The `CLAUDE_CODE_UA` version string must stay current — nightly CI checks via `scripts/check_upstream_versions.py`.

## TUI

- `core/crates/omegon/src/tui/mod.rs` owns top-level native TUI orchestration and remains large. Prefer the extracted owners for rendering, input, agent-event projection, semantic actions, slash routing, native I/O, Auspex, workspace context, and conversation/operation projections rather than adding new policy to the monolith.
- `om` defaults to an eight-row inline TUI with `Active` detail; `omegon` defaults to fullscreen with `Full` detail. Layout and detail are independent (`--tui`, `--ui`); legacy `om`/`lean`/`slim` detail values mean `Active`. `/ui terminal inline|fullscreen` changes the session base, and `/ui active|full` persists detail.
- Shared semantic projections live under `core/crates/omegon/src/surfaces/`. Keep producer/provenance independent from content form and avoid renderer-specific policy.
- Table rendering uses `markdown_display_width` for column measurement so Markdown emphasis/code markers do not distort padding.
- Routine captured TUI tests use the private headless PTY runner. Reserve native GUI trials for an explicitly selected compatibility session; do not launch terminal matrices on the operator's active desktop during routine iteration. Track and verify owned-window cleanup separately from child-process exit, and stop the matrix if cleanup fails.


## Current harness surfaces

- **Workbench**: pinned structured-work surface for active plan, cleave, delegate, and workstream summaries. It is operational state, not decoration. If Workbench contradicts the assistant's final reply, investigate/reconcile before claiming completion.
- **Semantic conversation surfaces**: shared projections live under `core/crates/omegon/src/surfaces/`. Keep producer/provenance independent from content form; TUI and ACP should consume semantic DTOs/projections rather than duplicating renderer logic.
- **Command registry**: operator commands should register through `CommandDefinition` with availability/safety metadata for TUI, CLI remote slash execution, and ACP where applicable. Avoid TUI-only slash arms unless the operation is truly UI-local.
- **Prompt and loop surfaces**: `/prompt` and `/loop` are intended registry commands across TUI/CLI/ACP. Prompt IDs are data resolved by those commands; do not register prompt IDs as top-level slash commands. Prompt/loop execution needs provenance and anti-prompt-injection safety checks.
- **ACP**: first-class rich-client surface for Zed/Flynt/future clients. ACP DTOs should derive from semantic surfaces or domain read models, not Ratatui/TUI structs.

## Codex integration

- Integration is optional and loads, in order, from `.codex/omegon-integration.toml` or `.omegon/codex.toml`. A generic `.codex/config.toml` does not enable it.
- Memory facts materialize to `{vault}/ai/memory/` on session end. Design nodes export to `{vault}/design/` by default.
- Facts referenced by vault notes get reinforced (decay timer reset) on sync.

## MCP

- MCP servers configured via `.omegon/mcp.toml` or plugin manifests. Resources and prompts discovered at connect time.
- Context injection capped at 10 items per category with TTL=50.

## k8s / containers

- `omegon run task.toml` — bounded headless tasks with structured JSON output. Exit codes: 0=done, 1=error, 2=exhausted, 3=timeout.
- `omegon serve` — long-lived daemon with WebSocket/IPC control plane, health probes at `/api/healthz` and `/api/readyz`.
- Workload matrix: `docs/design/k8s-workload-matrix.md` — tracks implementation status.

## Things to be careful with

- **Never fabricate URLs, client IDs, or API endpoints.** Research real values from provider documentation or source code. The Antigravity provider had fabricated credentials that wasted significant time.
- **Process cleanup is tree-scoped.** Timeout/cancellation paths that spawn commands must terminate the whole process group, not only the immediate shell. Under WSL this guarantee covers Linux descendants; Windows-host executables cross a lifecycle boundary and remain best-effort unless platform-specific control is added and tested.
- **`Settings::provider()` returns `String`** (not `&str`). It uses `infer_provider_id` — no hardcoded catch-all.
- **Skill frontmatter** — YAML (`---`) is canonical for portable/user-facing `SKILL.md` files; TOML (`+++`) remains supported for bundled and existing Omegon skills. `extract_description` handles both.
- **Extension `execute_tool` RPC** — extensions must implement this handler or the call returns a graceful error. The extension SDK is external; do not recreate the removed internal `omegon-extension` crate.
- **Memory/lifecycle features** have optional `codex_vault_path` — set via `with_codex_vault()` in `setup.rs`.
- **Plan/Workbench consistency** — never report "nothing pending" while the active Workbench plan still has active/todo items. Update, complete, skip, or clear it, or state the mismatch explicitly.
- **Logical commits** — split feature changes, rustfmt-only churn, and generated state changes into separate commits.
- **Avoid volatile facts in directives.** Counts, line totals, version strings, and inventories decay quickly; name the authoritative command or source unless the value itself is an enforced invariant.
