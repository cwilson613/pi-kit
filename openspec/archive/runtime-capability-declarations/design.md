# Design: Runtime capability declarations and registry integrity

## Approach

Add shared renderer-neutral declaration vocabulary in `omegon-traits` and an adapter/validator module in `omegon`. The adapter consumes existing `ToolDefinition` and `CommandDefinition` values. It does not participate in filtering or dispatch.

Capability identifiers use explicit kind namespaces (`tool:<name>`, `action:<canonical-name>`). Invocation bindings are separate records so command aliases do not create duplicate capability identities.

Registry validation returns all deterministic diagnostics in stable order rather than failing at the first conflict. The initial inventory is read-only and exists to establish parity before authority migrates in a later slice.

## Constraints

- No mutation of `DisabledTools` or `ToolInventorySnapshot` semantics.
- No changes to model schema projection or EventBus execution routing.
- Shared contracts remain serialization-compatible and runtime-neutral.
- Feature ownership must come from the registration boundary, not inferred from labels or descriptions.
- Existing command aliases resolve to one canonical action declaration.

## Initial file scope

- `core/crates/omegon-traits/src/lib.rs`
- `core/crates/omegon/src/capability_admission.rs`
- `core/crates/omegon/src/lib.rs`
- `core/crates/omegon/src/bus.rs`
