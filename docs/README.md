+++
id = "f7acbe5d-5a48-4077-bad2-54a7e08f8d6c"
tags = ["documentation", "index"]
aliases = ["docs-index"]
imported_reference = false

[publication]
enabled = false
visibility = "private"
+++

# Documentation Map

This directory is the durable project knowledge base for Omegon. It contains architecture notes, implementation plans, postmortems, and lifecycle records. It is not the same as the public docs site.

For user-facing docs, edit `site/src/pages/docs/` and the command snippets in `site/snippets/`.

## Start Here

- `README.md` at the repository root: product overview, install, core concepts, and source build path.
- `CONTRIBUTING.md`: branch policy, validation commands, release flow, and workspace layout.
- `docs/extensions.md`: canonical host-runtime guide for extension/plugin/MCP identity, trust, lifecycle, cleanup, and diagnostics. The standalone `omegon-extension-rs` repository owns SDK APIs.
- `docs/omegon-install.md`: distribution notes, Linux glibc caveats, and update contract.
- `docs/provider-credential-map.md`: provider auth and credential behavior.
- `docs/omegon-session.md`: session persistence behavior.
- `docs/cleave.md`: parallel worktree orchestration.
- `docs/sentry.md`: long-running task executor, triggers, budgets, and auto routing.
- `docs/n8n-sentry-submission.md`: planned external workflow submission API for n8n, Flynt, Auspex, and future protocol adapters.
- `docs/omegon-browser-extension.md`: native browser automation extension backed by Vercel agent-browser.
- `docs/armory-discovery.md`: unified discovery model for browsing upstream extensions, plugins, skills, and catalog agents.
- `docs/project-memory.md`: project memory behavior.
- [`docs/context-retention.md`](context-retention.md): token-budgeted compaction, complete exchanges, and retained-context limits.
- [`docs/project-instructions.md`](project-instructions.md): scoped AGENTS.md discovery, source completeness, and preparation errors.
- [`docs/mcp-phase-deadlines.md`](mcp-phase-deadlines.md): startup, inventory, and execution budgets with legacy fallback.
- [`docs/reconnect-parity-verification.md`](reconnect-parity-verification.md): reconnect fixes, approval replay, and duplicate-input evidence boundaries.
- `docs/openapi-tools.md`: project-local OpenAPI specs compiled into agent tools.
- `docs/prompt-and-user-command-surfaces.md`: reusable prompt definitions, `/prompt` routing, safety verdicts, and user-defined command aliases.
- [`docs/harness-architecture-parity/`](harness-architecture-parity/README.md): evidence-pinned architecture matrix, harness profiles, and philosophy/tradeoff analysis for OpenCode, Omegon, Pi, and DeepSeek Harness.
- [`docs/selective-kernel-decomposition.md`](selective-kernel-decomposition.md): adopted assessment of DeepSeek Harness's "everything is a plugin" philosophy and the selective decomposition of Omegon into a constitutional kernel, system modules, services, external contributions, content packs, and frontend adapters.
- [`docs/omegon-maintain.md`](omegon-maintain.md): Slice-zero contract for the independent maintenance executable, including commands, trust boundaries, mutation roots, deadlines, structured output, packaging, and deferred operations.

## Directory Boundaries

- `docs/`: durable architecture and implementation docs that should remain readable over time.
- `design/`: older design notes and exploratory material.
- `openspec/`: active and archived OpenSpec lifecycle artifacts.
- `site/`: public documentation site source and generated `dist/`.
- `ai/benchmarks/`: benchmark tasks and recorded runs.

When adding a new long-lived document, prefer `docs/` and include frontmatter. When adding public-facing guidance, update the Astro page in `site/src/pages/docs/` and use snippets for commands that appear in more than one place.

- `docs/acp-surface.md`: canonical ACP integration contract for Zed, Flynt, and external clients.
