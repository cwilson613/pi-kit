# Shared Traits Crate Directives

This file augments the repository-root `AGENTS.md` for work under `core/crates/omegon-traits/`.

## Ownership

`omegon-traits` defines contracts shared across the runtime and extracted crates: feature hooks, tools, typed bus events/requests, command metadata, semantic operation projections, and native IPC protocol types. It should contain vocabulary and protocol behavior, not application orchestration.

## Compatibility rules

- Treat public enums, serialized field names, defaults, and framing constants as compatibility-sensitive. Additive changes are preferred; breaking changes require coordinated producer/consumer updates and explicit migration rationale.
- Preserve producer/provenance as typed fields. Renderers must not infer delegate, cleave, assistant, peer, or tool identity from formatted text.
- Keep DTOs renderer-neutral. Terminal colors, widget geometry, HTML, and frontend-specific shortcuts do not belong here.
- `Feature`, `BusEvent`, and `BusRequest` form a directional interface. Avoid callbacks or dependencies that couple this crate back to the main binary.
- Legacy provider/context/session traits remain migration compatibility surfaces. Do not remove or subtly redefine them without updating every implementation and documenting the migration.
- IPC framing and transport limits are protocol contracts. Bound lengths before allocation, reject malformed frames deterministically, and retain same-user/socket-permission assumptions in tests and documentation.
- Serialized enums need unknown/evolution handling appropriate to their boundary. Never reorder behavior based on enum declaration order unless the order is itself specified.
- Keep dependencies minimal and runtime-neutral; this crate should not acquire provider clients, databases, TUI libraries, or application configuration.

## Validation

Run:

```bash
just test-crate omegon-traits
cargo test -p omegon-traits --locked
```

For contract changes, also test affected producers and consumers—normally `omegon`, plus any extracted crate implementing the changed trait. Add serialization round-trip and backward-compatibility fixtures for wire-shape changes.
