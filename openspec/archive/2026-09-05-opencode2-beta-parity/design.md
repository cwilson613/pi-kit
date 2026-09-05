# Immediate parity design

## Priority and boundaries

Implementation is complete. Delivery order was instruction discovery,
MCP phase deadlines, then reconnect and duplicate-action verification. Both local findings are source-confirmed in the
[comparison](../../../docs/opencode2-beta-parity.md). Effort estimates there are
relative judgments, not measured durations.

OpenCode remains implementation evidence. Omegon's prompt ownership, managed
services, session authority, route admission, and RBAC remain authoritative.
No frozen session payload, route lease, or compaction contract needs to change.

## 1. Complete instruction discovery

Primary owner: `core/crates/omegon/src/prompt.rs::load_project_directives` and its
callers. Inspect `find_repo_root` and existing prompt preparation before editing.

Discover every ancestor AGENTS.md from the active worktree root through cwd.
Render root first and nearest scope last, with source labels. Keep global
operator guidance in its existing owner. Project directives add to root policy;
ordering must not be described as permission to override immutable core rules.
Use the active worktree boundary, not the main checkout behind its `.git` file.
Do not scan sibling directories or descend below cwd in this slice.

Deduplicate canonical source paths. Preserve explicit symlinked policy files,
including shared targets outside the worktree; bound the discovery walk itself. If cwd is outside a Git worktree, preserve
cwd-only discovery. Missing files are normal; distinguish them from unreadable
files. An unreadable applicable file produces an actionable preparation error,
not silent fallback to a less specific file or an indefinite wait.

Remove the 4000-byte per-file truncation. Preserve complete UTF-8 content and
source boundaries. Inspect the existing request budget enforcement and route
preparation path: if complete required guidance cannot fit, return a bounded,
actionable error before dispatch. Do not create another tokenizer or a separate
configuration subsystem merely to replace this truncation.

This slice operates when existing prompt construction runs. It does not promise
live updates on every attempt, durable generations, or historical replay of
changed files. Those require a separate design. Record that boundary in tests
and operator documentation rather than implying full instruction-lifecycle parity.

## 2. MCP phase deadlines

Primary owner: `core/crates/omegon/src/plugins/mcp.rs::McpServerConfig`, connection,
inventory, and invocation paths; update applicable Pkl schema owners as needed.

Add optional `startup_timeout_secs`, `catalog_timeout_secs`, and
`execution_timeout_secs`. Each unset phase inherits `timeout_secs`; retain the
existing default when all fields are absent. Validate explicit phase budgets as
positive durations and reject conversion overflow. Inspect existing handling of
legacy zero before changing it; preserve legacy behavior or document an explicit
migration rather than silently changing accepted configuration.

Transport establishment and initialization share a startup deadline. Inventory
listing and pagination share a catalog deadline. Tool calls, prompt retrieval,
and resource reads use execution deadlines. Managed lifecycle policy may impose
a stricter outer deadline, but it must not accidentally reuse a shorter startup
budget to limit an already-ready tool invocation.

Progress does not extend the execution deadline. Cancellation can end an
operation earlier. Preserve existing process-tree cleanup and managed settlement
owners. A timeout of one call must not indiscriminately kill unrelated calls or
a shared server. If transport shutdown is necessary, use the existing lifecycle
owner and surface its effect. Remote cancellation is not proof that remote work
stopped. Diagnostics identify phase, effective budget, and cleanup outcome.

Legacy servers with neither startup nor catalog overrides retain completed
inventory when optional discovery stalls after tools are loaded. Explicit
startup/catalog overrides select complete-catalog readiness. Shutdown removes
connections from the registry under a short lock before awaiting their settlement.

Use deterministic fake MCP servers and controlled time to test deadlines without
minute-long sleeps. Preserve existing resource/prompt injection limits and TTLs.

## 3. Reconnect and duplicate-action verification

Test snapshot/live-event handoff for both WebSocket routes. Subscribe before
building or sending the initial snapshot, so completion during an awaited send
remains observable. Retain existing authoritative queue and supervisor recovery.

Verify repeated durable admission identities across authority reopen, approval
recovery, and detached delegate results. Distinguish web-owned pending tool
approvals from cleave approvals and preserve the existing first-consumer
TUI/web responder ownership. Record unsupported transport retry identity and
client-detach versus daemon-restart behavior explicitly.

## Validation and landing

Each slice starts with its focused failing regressions and lands independently.
Use `just test-crate omegon` plus `just clippy-changed` for an isolated omegon
change. Shared contracts or multi-crate changes require `just test-commit`.
Run applicable schema checks when Pkl changes. Do not rerun unrelated provider
campaigns unless the actual diff affects their contracts.

Exercise current source through `just run`: inspect model-bound instructions in
a nested temporary worktree, and use a fake MCP server with short independent
budgets. Record current build identity, observable result, and cleanup evidence.
Use `just link` only if installed asset identity is necessary.

Close each task only after its validation completes. Archive this change after
all three workstreams satisfy their scenarios; deferred roadmap items do not block it.
The design update itself requires OpenSpec validation and `git diff --check`.
