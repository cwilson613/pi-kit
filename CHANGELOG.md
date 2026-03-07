# Changelog

All notable changes to pi-kit are documented here.
Format: [Keep a Changelog](https://keepachangelog.com/). Versioning: [Semantic Versioning](https://semver.org/).

## [0.1.3] - 2026-03-07

### Added

- **Non-capturing dashboard overlay** — new `panel` mode renders the dashboard as a persistent side panel that doesn't steal keyboard input, using pi 0.57.0's `nonCapturing` overlay API. `focused` mode enables interactive navigation within the panel.
- **4-state dashboard cycle** — `/dashboard` now cycles through `compact → raised → panel → focused`. Direct subcommands: `/dashboard panel`, `/dashboard focus`, `/dashboard open` (legacy modal).
- **Tab completions** for `/dashboard` subcommands (`compact`, `raised`, `panel`, `focus`, `open`).
- **Footer `/dashboard` hint** — compact footer now shows `/dashboard` for discoverability.

### Changed

- Dashboard keybind changed from `ctrl+shift+b` to `` ctrl+` `` — the previous binding was intercepted by Kitty terminal's default keymap (`move_window_backward`) and never reached pi.
- Upgraded `@mariozechner/pi-coding-agent` and `@mariozechner/pi-ai` to `^0.57.0`.

### Fixed

- Dashboard keybind was silently non-functional due to Kitty terminal default keymap collision.

## [0.1.2] - 2026-03-07

### Added

- **Version-check extension** — polls GitHub releases on session start and hourly. Notifies operator to run `pi update` when a newer release exists. Respects `PI_SKIP_VERSION_CHECK` and `PI_OFFLINE` env vars.

### Fixed

- Test command glob now includes root-level `extensions/*.test.ts` files (were silently missed by `**` glob).

### Changed

- README documents main-branch tracking limitation with link to [#5](https://github.com/cwilson613/pi-kit/issues/5).

## [0.1.1] - 2026-03-07

### Added

- **Scenario-first task generation** — cleave child tasks are now matched to spec scenarios using 3-tier priority: spec-domain annotations (`<!-- specs: domain -->`) → file scope matching → word-overlap fallback. Prevents cross-cutting spec scenarios (e.g., RBAC enforcement) from falling between children when tasks are split by file layer.
- **Orphan scenario auto-injection** — any spec scenario matching zero children is automatically injected into the closest child with a `⚠️ CROSS-CUTTING` marker for observability.
- **`TaskGroup.specDomains`** — parsed from `<!-- specs: ... -->` HTML comments in tasks.md group headers for deterministic scenario-to-child mapping.
- **`matchScenariosToChildren`** — exported function for pre-computing scenario assignments across all children with orphan detection.

### Fixed

- Domain matching is now path-segment-aware (`relay` no longer matches `relay-admin/permissions`).
- Scope matching uses word-boundary regex instead of substring (prevents `utils.py` matching "utility").
- `ChildPlan.specDomains` normalized to required `string[]` (was optional, causing type inconsistency with `TaskGroup`).

### Changed

- `buildDesignSection` in workspace.ts uses pre-computed scenario assignments instead of per-child word-overlap heuristic.
- `skills/openspec/SKILL.md` updated with scenario-first grouping guidance and annotation examples.
- `skills/cleave/SKILL.md` updated with annotation syntax and orphan behavior documentation.

## [0.1.0] - 2026-03-07

Initial public release.

### Added

- **OpenSpec extension** — spec-driven development lifecycle: propose → spec → design → tasks → verify → archive. Given/When/Then scenarios as acceptance criteria. Delta-spec merge on archive. API contract derivation from scenarios (`api.yaml`).
- **Design Tree extension** — structured design exploration with persistent markdown documents. Frontmatter-driven status tracking, open question syncing, branching from questions, and OpenSpec bridge (`/design implement` scaffolds change from decided node).
- **Cleave extension** — recursive task decomposition with parallel execution in git worktrees. Complexity assessment, OpenSpec integration (tasks.md as split plan, design context enrichment, task completion writeback). Code assessment: `/assess cleave` (adversarial + auto-fix), `/assess diff` (review), `/assess spec` (validate against scenarios + API contract), `/assess complexity`.
- **Project Memory extension** — persistent cross-session knowledge in SQLite+WAL. 11 tools for store/recall/query/supersede/archive/connect/compact/episodes/focus/release/search-archive. Semantic retrieval via Ollama embeddings (FTS5 fallback). Background fact extraction. Episodic session narratives. JSONL export/import with `merge=union` for git sync.
- **Local Inference extension** — delegate sub-tasks to Ollama models at zero API cost. Auto-discovers available models on session start.
- **Offline Driver extension** — switch driving model from cloud to local Ollama when connectivity drops. Auto-selects best available model (Nemotron, Devstral, Qwen3).
- **Model Budget extension** — switch model tiers (opus/sonnet/haiku) and thinking levels (off/minimal/low/medium/high) to match task complexity and conserve API spend.
- **Render extension** — FLUX.1 image generation via MLX on Apple Silicon, D2 diagram rendering, Excalidraw JSON-to-PNG.
- **Web Search extension** — multi-provider search (Brave, Tavily, Serper) with quick/deep/compare modes and deduplication.
- **MCP Bridge extension** — connect external MCP servers as pi tools via stdio transport.
- **Secrets extension** — resolve secrets from env vars, shell commands, or system keychains via declarative `@secret` annotations.
- **Auth extension** — authentication status, diagnosis, and refresh across git, GitHub, GitLab, AWS, k8s, OCI registries.
- **Chronos extension** — authoritative date/time from system clock, eliminates AI date calculation errors.
- **View extension** — inline file viewer for images, PDFs, documents, and syntax-highlighted code.
- **Auto-compact extension** — context pressure monitoring with automatic compaction.
- **Defaults extension** — auto-deploys AGENTS.md and theme on first install with content-hash guard to prevent overwrites.
- **Distill extension** — context distillation for session handoff.
- **Session Log extension** — append-only structured session tracking.
- **Status Bar extension** — severity-colored context gauge with memory usage and turn counter.
- **Terminal Title extension** — dynamic tab titles for multi-session workflows.
- **Spinner Verbs extension** — themed loading messages.
- **Style extension** — Verdant design system reference.
- **Shared State extension** — cross-extension state sharing.
- **Skills**: openspec, cleave, git, oci, python, rust, style.
- **Prompt templates**: new-repo, oci-login.
- **Global directives**: attribution policy (no AI co-author credit), spec-first development methodology, API contract requirement (OpenAPI 3.1 derived from scenarios), runtime validation middleware guidance, completion standards, memory sync rules, branch hygiene.
- **Documentation**: README with architecture diagram, spec pipeline diagram, memory lifecycle diagram. CONTRIBUTING.md with branching policy, memory sync architecture, and cleave branch cleanup.

### Security

- Path traversal hardening in view and render extensions.
- Command injection prevention in cleave worktree operations.
- Design tree node ID validation.
