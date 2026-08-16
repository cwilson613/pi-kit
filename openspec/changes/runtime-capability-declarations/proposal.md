---
state: implementing
---
# Runtime capability declarations and registry integrity

## Intent

Introduce an additive, authority-neutral declaration and inventory layer for runtime tools and operator commands so every registered invocation has stable ownership and registry drift is detected before capability admission becomes authoritative.

## Scope

- Define runtime capability identities, kinds, owners, invocation bindings, and declaration diagnostics.
- Adapt existing tool and command definitions into declarations without changing projection or dispatch.
- Validate duplicate identities and invocation vocabulary, missing owners, dangling aliases/groups, and unsupported bindings.
- Expose a read-only inventory for diagnostics and parity tests.

## Non-goals

- Replacing `DisabledTools`, posture policy, tool schema projection, or dispatch authority.
- Migrating skills, context contributions, IPC/WebSocket actions, or binary feature composition.
- Adding dynamic admission or generation leases.

## Success criteria

- Existing tools and built-in commands project deterministic declarations with stable owners.
- Registry integrity failures are structured and directly testable.
- Existing callable tool behavior is unchanged.
