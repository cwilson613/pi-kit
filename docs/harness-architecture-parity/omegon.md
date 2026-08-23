+++
id = "2a40421d-1a16-4c5c-8ee0-89fc1b39f1cf"
kind = "document"
title = "Omegon architecture profile"
status = "active"
tags = ["architecture", "harness", "omegon"]
aliases = ["omegon-architecture-profile"]
imported_reference = false

[publication]
enabled = false
visibility = "private"
+++

# Omegon architecture profile

[Collection index](README.md) | [Matrix](matrix.md) | [Philosophies](philosophies.md)

## Identity and baseline

This profile describes current source at commit `a443b9b6`, 2026-08-17,
workspace version `0.29.0-dev`. Current code and crate directives are authority;
older architecture and migration documents are not treated as shipped behavior.

The root [`Cargo.toml`](../../Cargo.toml) declares the workspace. The main crate
[`core/crates/omegon`](../../core/crates/omegon) is the integration binary; its
[`AGENTS.md`](../../core/crates/omegon/AGENTS.md) defines ownership and runtime
invariants.

## Architecture

Omegon is a Rust-native runtime with a large integration binary and extracted
domain crates for Git operations, memory, secrets, lifecycle, RBAC, skills,
shared contracts, work models/runtime, and web extraction. The main binary owns
the provider loop, runtime composition, tools, permissions, TUI, ACP, daemon,
control planes, and extension integration.

The interactive execution path is:

```text
operator surface
  -> runtime prompt/queue authority
  -> provider stream
  -> canonical conversation update
  -> EventBus tool dispatch / feature events
  -> next turn or authoritative completion
```

Primary source entry points:

- loop: `core/crates/omegon/src/loop.rs`
- composition: `core/crates/omegon/src/setup.rs`
- tools/features: `core/crates/omegon/src/bus.rs`
- conversation: `core/crates/omegon/src/conversation.rs`
- interactive authority: `core/crates/omegon/src/interactive_coordinator.rs`
- providers/routes: `core/crates/omegon/src/providers.rs` and `route.rs`
- shared contracts: `core/crates/omegon-traits/src/lib.rs`

## Capabilities

| Area | Baseline behavior |
|---|---|
| Providers | Native Rust bridges plus compatible routes, explicit route state, narrow same-family fallback, provider-specific schema normalization. |
| Tools | Broad feature-composed inventory with core/situational groups and progressive model disclosure. |
| Context | Canonical versus LLM-facing history, semantic decay skeletons, automatic and overflow compaction, retained intent. |
| Memory | Separate durable facts, confidence/decay, typed edges, episodes, optional vectors, and vault synchronization. |
| Planning | Durable plan model and Workbench projection, plus OpenSpec/design/milestone lifecycle state machines. |
| Agents | Delegate children and cleave worktree orchestration with dependency waves, bounded concurrency, merge, review, and typed operation status. |
| Extensions | Native/OCI JSON-RPC extensions, MCP, OpenAPI-generated tools, portable skills, external SDK contract. |
| Interfaces | TUI, CLI, bounded headless run, ACP, native IPC, HTTP/WebSocket control plane, daemon, and sentry executor. |
| Code intelligence | Tree-sitter extraction, SQLite/BM25 indexing, and code search; no current LSP client. |

## Authority model

Omegon does not have one universal session database. Different durable concerns
have distinct authorities:

- resumable snapshots persist an LLM-facing conversation projection, intent,
  observations, and compaction state; load reconstructs a canonical recent tail
  rather than the original canonical transcript verbatim;
- interactive runtime supervisor/queue state owns active-turn admission and
  completion; bounded run calls the loop directly and daemon admission has a
  separate, less complete path;
- session-local plan state persists inside the conversation intent snapshot;
- lifecycle plans derive from Git/OpenSpec/design artifacts;
- Workbench reconciles and projects session-local and lifecycle plan state;
- Git-native Markdown/OpenSpec artifacts own lifecycle meaning;
- the lifecycle store enforces transitions and records audit state;
- semantic memory owns reusable project facts and relationships;
- Git owns repository and worktree state.

This permits richer orchestration than a transcript-only harness, but every
boundary needs explicit reconciliation. The operator-agency invariant in
`core/crates/omegon/AGENTS.md` exists because advisory presentation events must
not remain the sole authority for whether another turn can start.

## Security and containment

The permission evaluator defines Lex, persona, project, and session layers with
deny-overrides semantics, but current settings integration populates project
policy only; the other policy layers remain a scaffold. Paths are canonicalized
against workspace/trusted roots. Secrets use dedicated resolution, encrypted
storage, and redaction.
Native extensions start with a cleared environment and receive declared secrets
after protocol handshake. OCI profiles can isolate full sessions or children.

Important qualifications:

- absence of a matching permission rule for a known, graph-admitted capability
  currently defaults to allow at the permission layer;
- unknown owners, capabilities, effects, schemas, and provenance receive no
  privileged execution lease, so the permission default cannot admit an
  undeclared tool;
- `--dangerously-bypass-permissions` disables filesystem-boundary prompts and
  some command confirmations, but not configured policy, RBAC, extension
  policy, or sandbox enforcement;
- process-group cleanup is strong on Unix and weaker on non-Unix systems;
- several network/plugin trust boundaries remain configuration-dependent.

Relevant source:

- `core/crates/omegon/src/permissions.rs`
- `core/crates/omegon/src/tools/mod.rs`
- `core/crates/omegon/src/tools/bash.rs`
- `core/crates/omegon/src/extensions/mod.rs`
- `core/crates/omegon/src/sandbox_runtime/`
- `core/crates/omegon-rbac/src/lib.rs`
- `core/crates/omegon-secrets/`

## Philosophy

Omegon optimizes for durable intent, explicit runtime authority, provenance,
operator agency, controlled parallelism, and recovery across long-running work.
It treats the coding task as an orchestration of conversations, state changes,
and child operations rather than as one transcript. Semantic projections are
intended to let TUI, ACP, IPC, and Web clients consume common domain state
without owning execution policy.

## Material limitations at the baseline

- Interactive runtime extraction is incomplete: the compiled coordinator
  coexists with standalone supervisor/turn modules that are not the authority.
- Inference inventory remains shadow-gated rather than the default route owner.
- Predictive and loop-level compaction policies overlap.
- Provider-neutral `styrene-work-*` crates are not yet composed into the main
  Workbench implementation.
- The daemon does not fully route work by caller identity, Web cancellation is
  ineffective, and image attachments are ignored in one headless path.
- TOML task specs are used, but the separate `--manifest` agent-manifest
  override is accepted and ignored; token budgets are observed rather than
  enforced.
- Resume is implemented for interactive/standalone and ACP paths, while bounded
  run has no resume/save contract and serve does not restore its previous
  default session at startup.
- LSP is absent; codescan provides a different structural/search capability.
- Cleave is Git-worktree-specific.
- `omegon-codescan` is a live path dependency but is absent from the root
  workspace member list.
- The integration binary remains broad, so an instability can affect the
  default UI, loop, control, and recovery surfaces together.
