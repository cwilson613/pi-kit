# pi-kit

A batteries-included extension package for the [pi coding agent](https://github.com/badlogic/pi-mono). Adds persistent project memory, spec-driven development, local LLM inference, image generation, web search, parallel task decomposition, a live dashboard, and quality-of-life tools — all loadable with a single install.

```bash
pi install https://github.com/cwilson613/pi-kit
```

> **Note:** `pi install` and `pi update` track the `main` branch. There is no tag-pinned install yet — see [#5](https://github.com/cwilson613/pi-kit/issues/5). The version-check extension notifies you when a new release is available.

## Architecture

![pi-kit Architecture](docs/img/architecture.png)

pi-kit extends the pi agent with **27 extensions**, **12 skills**, and **4 prompt templates** — loaded automatically on session start.

### Development Methodology

pi-kit enforces **spec-first development** for non-trivial changes:

![Spec-Driven Pipeline](docs/img/spec-pipeline.png)

The full lifecycle: **design → propose → spec → plan → implement → verify → archive**. Given/When/Then scenarios are the source of truth for correctness — code implements the specs, not the reverse.

## Extensions

### 📋 OpenSpec

Spec-driven development lifecycle — proposal → specs → design → tasks workflow with delta-spec merge on archive.

- **Tool**: `openspec_manage`
- **Commands**: `/opsx:propose`, `/opsx:spec`, `/opsx:ff`, `/opsx:status`, `/opsx:verify`, `/opsx:archive`, `/opsx:sync`
- **Lifecycle stages**: proposed → specified → planned → implementing → verifying → archived
- **API contracts**: When a change involves a network API, derives an OpenAPI 3.1 spec from Given/When/Then scenarios; `/assess spec` validates implementation against it
- Integrates with [OpenSpec CLI](https://github.com/Fission-AI/OpenSpec) profiles

### 🪓 Cleave

Parallel task decomposition with dependency-ordered wave dispatch in isolated git worktrees.

- **Tools**: `cleave_assess` (complexity evaluation), `cleave_run` (parallel dispatch)
- **Commands**: `/cleave <directive>`, `/assess cleave`, `/assess diff`, `/assess spec`
- **OpenSpec integration**: Uses `tasks.md` as the split plan when `openspec/` exists, enriches child tasks with design decisions and spec acceptance criteria, reconciles task completion on merge, guides through verify → archive
- **Skill-aware dispatch**: Auto-matches skill files to children based on file scope patterns (e.g. `*.py` → python, `Containerfile` → oci). Annotations (`<!-- skills: python, k8s -->`) override auto-matching
- **Model tier routing**: Each child resolves an execution tier — explicit annotation > skill-based hint > default. Provider-neutral tier labels resolve to concrete models through the session routing policy
- **Adversarial review loop** (opt-in via `review: true`): After each child completes, an opus-tier reviewer checks for bugs, security issues, and spec compliance. Severity-gated: nits→accept, warnings→1 fix iteration, criticals→2 fixes then escalate, security→immediate escalate. Churn detection bails when >50% of issues reappear between rounds
- **Large-run preflight**: Asks which provider to favor before expensive dispatches, preventing mid-run subscription exhaustion

### 🌲 Design Tree

Structured design exploration with persistent markdown documents — the upstream of OpenSpec.

- **Tools**: `design_tree` (query), `design_tree_update` (create/mutate nodes)
- **Commands**: `/design list`, `/design new`, `/design update`, `/design branch`, `/design decide`, `/design implement`
- **Document structure**: Frontmatter (status, tags, dependencies, open questions) + sections (Overview, Research, Decisions, Open Questions, Implementation Notes)
- **OpenSpec bridge**: `design_tree_update` with `action: "implement"` scaffolds `openspec/changes/<node-id>/` from a decided node's content; `/cleave` executes it
- **Full pipeline**: design → decide → implement → `/cleave` → `/assess spec` → archive

### 🧠 Project Memory

Persistent, cross-session knowledge stored in SQLite. Accumulates architectural decisions, constraints, patterns, and known issues — retrieved semantically each session.

- **11 tools**: `memory_store`, `memory_recall`, `memory_query`, `memory_supersede`, `memory_archive`, `memory_connect`, `memory_compact`, `memory_episodes`, `memory_focus`, `memory_release`, `memory_search_archive`
- **Semantic retrieval**: Embedding-based search via Ollama (`qwen3-embedding`), falls back to FTS5 keyword search
- **Background extraction**: Auto-discovers facts from tool output without interrupting work
- **Episodic memory**: Generates session narratives at shutdown for "what happened last time" context
- **Global knowledge base**: Cross-project facts at `~/.pi/memory/global.db`
- **Git sync**: Exports to JSONL for version-controlled knowledge sharing across machines

![Memory Lifecycle](docs/img/memory-lifecycle.png)

### 📊 Dashboard

Live status panel showing design tree, OpenSpec changes, cleave dispatch, and git branches at a glance.

- **Commands**: `/dash` (toggle compact ↔ raised), `/dashboard` (open side panel)
- **Compact mode**: Single footer line — design/openspec/cleave summaries + context gauge
- **Raised mode**: Full-width expanded view (toggle with `/dash`)
  - Git branch tree rooted at repo name, annotated with linked design nodes
  - Two-column split at ≥120 terminal columns: design tree + cleave left, OpenSpec right
  - Context gauge · model · thinking level in shared footer zone
  - No line cap — renders as much content as needed
- **Keyboard**: `Ctrl+Shift+B` toggles raised/compact

### ⚔️ Effort Tiers

Single global knob controlling the inference intensity across the entire harness. Seven named tiers using provider-neutral labels — tier labels resolve to concrete model IDs from whichever provider (Anthropic or OpenAI) the session routing policy prefers.

| Tier | Name | Driver | Thinking | Review |
|------|------|--------|----------|--------|
| 1 | **Servitor** | local | off | local |
| 2 | **Average** | local | minimal | local |
| 3 | **Substantial** | sonnet | low | sonnet |
| 4 | **Ruthless** | sonnet | medium | sonnet |
| 5 | **Lethal** | sonnet | high | opus |
| 6 | **Absolute** | opus | high | opus |
| 7 | **Omnissiah** | opus | high | opus |

- `/effort <name>` — switch tier mid-session
- `/effort cap` — lock current tier as ceiling; agent cannot self-upgrade past it
- `/effort uncap` — remove ceiling lock
- Affects: driver model, thinking level, extraction, compaction, cleave child floor, review model

### 🤖 Local Inference

Delegate sub-tasks to locally running LLMs via Ollama — zero API cost.

- **Tools**: `ask_local_model`, `list_local_models`
- **Commands**: `/local-models`, `/local-status`
- Auto-discovers available models on session start

### 🔌 Offline Driver

Switch the driving model from cloud to a local Ollama model when connectivity drops or for fully offline operation.

- **Tool**: `switch_to_offline_driver`
- Auto-selects best available model from a hardware-aware preference list
- Model registry in `extensions/lib/local-models.ts` — one file to update when new models land
- Covers: 64GB (70B), 32GB (32B), 24GB (14B/MoE-30B), 16GB (8B), 8GB (4B)

### 💰 Model Budget

Switch model tiers to match task complexity and conserve API spend. Tier labels are provider-neutral — resolved at runtime through the session routing policy.

- **Tool**: `set_model_tier` — `opus` / `sonnet` / `haiku` / `local`
- **Tool**: `set_thinking_level` — `off` / `minimal` / `low` / `medium` / `high`
- Downgrade for routine edits, upgrade for architecture decisions
- Respects effort tier cap — cannot upgrade past a locked ceiling

### 🎨 Render

Generate images and diagrams directly in the terminal.

- **FLUX.1 image generation** via MLX on Apple Silicon — `generate_image_local`
- **D2 diagrams** rendered inline — `render_diagram`
- **Native SVG/PNG diagrams** for canonical motifs (pipeline, fanout, panel-split) — `render_native_diagram`
- **Excalidraw** JSON-to-PNG rendering — `render_excalidraw`

### 🔍 Web Search

Multi-provider web search with deduplication.

- **Tool**: `web_search`
- **Providers**: Brave, Tavily, Serper (Google)
- **Modes**: `quick` (single provider, fastest), `deep` (more results), `compare` (all providers, best for research)

### 🗂️ Tool Profiles

Enable or disable tools and switch named profiles to keep the context window lean.

- **Tool**: `manage_tools`
- **Command**: `/profile [name|reset]`
- Pre-built profiles for common workflows; per-tool enable/disable for fine-grained control

### 📖 Vault

Markdown viewport for project documentation — serves docs with wikilink navigation and graph view.

- **Command**: `/vault`

### 🔐 Secrets

Resolve secrets from environment variables, shell commands, or system keychains — without storing values in config.

- Declarative `@secret` annotations in extension headers
- Sources: `env:`, `cmd:`, `keychain:`

### 🌐 MCP Bridge

Connect external MCP (Model Context Protocol) servers as native pi tools.

- Bridges MCP tool schemas into pi's tool registry
- Stdio transport for local MCP servers

### 🔧 Utilities

| Extension | Description |
|-----------|-------------|
| `bootstrap` | First-time setup — check/install dependencies, capture operator preferences (`/bootstrap`, `/refresh`) |
| `chronos` | Authoritative date/time from system clock — eliminates AI date math errors |
| `01-auth` | Auth status, diagnosis, and refresh across git, GitHub, GitLab, AWS, k8s, OCI (`/auth`, `/whoami`) |
| `view` | Inline file viewer — images, PDFs, docs, syntax-highlighted code |
| `distill` | Context distillation for session handoff (`/distill`) |
| `session-log` | Append-only structured session tracking |
| `auto-compact` | Context pressure monitoring with automatic compaction |
| `defaults` | Deploys `AGENTS.md` and theme on first install; content-hash guard prevents overwriting customizations |
| `terminal-title` | Dynamic tab titles showing active cleave runs and git branch |
| `spinner-verbs` | Warhammer 40K-themed loading messages |
| `style` | Verdant design system reference (`/style`) |
| `version-check` | Polls GitHub releases hourly, notifies when a new pi-kit release is available |

## Skills

Skills provide specialized instructions the agent loads on-demand when a task matches.

| Skill | Description |
|-------|-------------|
| `openspec` | OpenSpec lifecycle — writing specs, deriving API contracts, generating tasks, verifying implementations |
| `cleave` | Task decomposition, code assessment, OpenSpec lifecycle integration |
| `git` | Conventional commits, semantic versioning, branch naming, changelogs |
| `oci` | Container and artifact best practices — Containerfile authoring, multi-arch builds, registry auth |
| `python` | Project setup (src/ layout, pyproject.toml), pytest, ruff, mypy, packaging, venv |
| `rust` | Cargo, clippy, rustfmt, Zellij WASM plugin development |
| `typescript` | Strict typing, async patterns, error handling, node:test conventions for pi-kit |
| `pi-extensions` | pi extension API — `registerCommand`, `registerTool`, event handlers, TUI context, common pitfalls |
| `pi-tui` | TUI component patterns — `Component` interface, overlays, keyboard handling, theming, footer/widget APIs |
| `security` | Input escaping, injection prevention, path traversal, process safety, secrets management |
| `style` | Verdant color system, typography, spacing — shared across TUI, D2 diagrams, and generated images |
| `vault` | Obsidian-compatible markdown conventions — wikilinks, frontmatter, vault-friendly file organization |

## Prompt Templates

Pre-built prompts for common workflows:

| Template | Description |
|----------|-------------|
| `new-repo` | Scaffold a new repository with conventions |
| `init` | First-session environment check — orient to a new project directory |
| `status` | Session orientation — load project state and show what's active |
| `oci-login` | OCI registry authentication |

## Requirements

- [pi coding agent](https://github.com/badlogic/pi-mono) (`@mariozechner/pi-coding-agent` ≥ 0.57)
- **Optional**: [Ollama](https://ollama.ai) — for local inference, offline mode, and semantic memory search
- **Optional**: [d2](https://d2lang.com) — for diagram rendering
- **Optional**: [mflux](https://github.com/filipstrand/mflux) — for FLUX.1 image generation on Apple Silicon
- **Optional**: API keys for web search (Brave, Tavily, or Serper)

Run `/bootstrap` after install to check dependencies and configure preferences.

## License

ISC
