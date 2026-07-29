# Main Omegon Crate Directives

This file augments the repository-root `AGENTS.md` for work under `core/crates/omegon/`.

## Ownership

This crate is the integration binary. It owns agent-loop composition, providers, tools, permissions, TUI, ACP, daemon/control-plane adapters, extensions/plugins, semantic surfaces, and workspace/runtime coordination. Extracted crates own their domain contracts and engines; do not duplicate those implementations here for convenience.

## Where changes belong

- Shared client/renderer DTOs and feature/tool/event contracts belong in `omegon-traits`.
- Durable memory behavior belongs in `omegon-memory`; this crate owns setup and operator integration.
- Lifecycle transition rules belong in `omegon-opsx`; this crate owns Markdown artifact adapters and surfaces.
- Provider-neutral work contracts and aggregation belong in `styrene-work-model` / `styrene-work-runtime`.
- Shared semantic projections belong under `src/surfaces/`; TUI, ACP, web, and IPC adapters should consume them rather than inventing per-frontend policy.
- `src/tui/mod.rs` is an orchestration owner, not the default home for new behavior. Prefer the relevant extracted TUI module or a semantic surface.

## Runtime invariants

- `setup.rs` composes the live feature/runtime substrate. New capabilities must identify their owner, registration path, context/provenance behavior, and non-interactive surface behavior.
- Commands register through `CommandDefinition` unless truly frontend-local. Preserve availability, safety, provenance, and presentation metadata across TUI/CLI/ACP consumers.
- Keep provider identity explicit. Normalize tool schemas through `tool_schema.rs`; do not silently infer unsupported provider capabilities.
- Process timeout/cancellation must clean up the entire owned process tree. Never replace argument-array spawning with shell interpolation.
- Keep producer/provenance distinct from rendered content. Do not route operator surfaces by sniffing formatted body text.
- Permission decisions must preserve raw intent, path dialect/environment context, and the workspace boundary; avoid eager conversion that destroys Windows/WSL path meaning.

## Validation

Use the narrowest relevant tests during iteration. Typical commands:

```bash
just test-filter "focused_test_name"
cargo test -p omegon --test <integration_test>
just test-commit
just clippy-changed
```

For broad changes across setup, events, commands, permissions, providers, or multiple frontends, run the root-level broad gates. Exercise the actual runtime with `just run` or the installed launcher when behavior depends on TUI/process/tool integration.
