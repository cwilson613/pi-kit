# OpenSpec Lifecycle Crate Directives

This file augments the repository-root `AGENTS.md` for work under `core/crates/omegon-opsx/`.

## Ownership

`omegon-opsx` owns the typed OpenSpec/design/release lifecycle state machine and its state-store abstraction. It does not own Markdown scaffolding, command presentation, Workbench rendering, or the repository's `openspec/changes/**` authoring workflow; those integrations live in the main `omegon` crate and lifecycle tooling.

## Lifecycle invariants

- State transitions are policy. Route them through `Lifecycle` rather than mutating status fields in callers or stores.
- Invalid transitions must fail without partially mutating persisted state.
- `StateStore` implementations must agree on observable semantics. Use `MemoryStore` as a test double, not as a second policy implementation.
- JSON persistence is git-native durable state. Writes must be deterministic and crash-safe; prefer atomic replacement and preserve readable prior state on failure.
- Additive fields need serde defaults when older state files can omit them. Renames/removals require migration fixtures and coordinated consumers.
- Keep design-node, change, decision, and milestone vocabularies distinct. Do not collapse states merely because two frontends display the same label.
- The crate must not parse task Markdown or infer completion from prose. Callers reconcile artifacts, then submit explicit typed lifecycle operations.
- Errors should identify the rejected transition or store operation without leaking unrelated filesystem content.

## Validation

Run:

```bash
just test-crate omegon-opsx
cargo test -p omegon-opsx --locked
```

Every transition change needs table-driven coverage for the allowed path, rejected predecessor/successor states, and no-mutation-on-error behavior. Persistence changes need reopen and legacy-fixture tests. Run affected `omegon` lifecycle/command tests when public types or transition semantics change.
