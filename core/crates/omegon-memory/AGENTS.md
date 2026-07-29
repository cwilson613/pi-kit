# Memory Crate Directives

This file augments the repository-root `AGENTS.md` for work under `core/crates/omegon-memory/`.

## Ownership

`omegon-memory` owns durable memory types and storage behavior: backend contracts, SQLite and in-memory implementations, decay/confidence, embeddings and vectors, rendering, retrieval/service behavior, and Codex-vault synchronization. The main `omegon` crate owns runtime setup and operator-facing registration.

## Boundaries and invariants

- `MemoryBackend` is the storage abstraction. Keep behavior consistent between SQLite and `InMemoryBackend`; backend-specific shortcuts must not leak into callers.
- `types.rs` is the canonical persisted/wire vocabulary. Treat schema changes as migrations: preserve deserialization compatibility or add an explicit migration and fixtures.
- SQLite mutations that span records, edges, vectors, or metadata must be atomic. Keep foreign-key behavior and indexes covered by tests.
- Decay/confidence and reinforcement are domain semantics, not rendering concerns. Avoid hidden confidence changes in import/export code.
- Vector search is an optional retrieval signal. Preserve deterministic non-vector behavior when embeddings are unavailable.
- Vault sync is a bidirectional filesystem boundary. Validate paths, avoid following content outside the configured vault, make repeated sync idempotent, and never infer deletion from a transient read failure.
- `provider.rs` is feature-gated integration glue. Core storage and service code must continue to compile without the `agent` feature.
- Do not log fact content, embeddings, vault contents, or credentials at info-level diagnostics.

## Validation

Run focused tests while iterating, then the crate gate:

```bash
just test-crate omegon-memory
cargo test -p omegon-memory --all-features --locked
```

Run main-crate integration tests too when changing `MemoryProvider`, public re-exports, vault setup, or shared trait implementations. Persistence changes require round-trip/reopen tests; vault changes require idempotency and path-boundary tests.
