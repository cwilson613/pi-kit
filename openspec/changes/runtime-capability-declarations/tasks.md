# Runtime capability declarations — Tasks

## 1. Shared declaration contracts
<!-- specs: runtime-capabilities/declarations -->

- [x] 1.1 Add stable capability ID, kind, owner, invocation binding, declaration, group, and diagnostic types.
- [x] 1.2 Add constructor and serialization round-trip tests in `omegon-traits`.

## 2. Registry adapters and integrity validation
<!-- specs: runtime-capabilities/declarations -->

- [x] 2.1 Adapt owned tool and command definitions into deterministic declarations.
- [x] 2.2 Validate duplicate IDs, ambiguous invocation bindings, missing owners, and dangling group members.
- [x] 2.3 Add focused validator tests for every diagnostic and valid alias behavior.

## 3. Read-only runtime integration
<!-- specs: runtime-capabilities/declarations -->

- [x] 3.1 Build the declaration inventory from EventBus registration ownership without changing filtering or dispatch.
- [x] 3.2 Add parity coverage proving callable tools and execution behavior remain legacy-owned.
- [x] 3.3 Run crate tests and changed-file clippy.
