---
state: proposed
---
# Selective Omegon kernel decomposition

## Intent

Turn Omegon's existing contribution seams into one selectively composable runtime without delegating constitutional authority to ordinary plugins. Establish an independently runnable maintenance artifact, durable per-session runtime authority, a validated contribution graph, and one crash-consistent privileged invocation path before extracting optional domains from the integration binary.

## Context

Omegon already composes statically linked `Feature` implementations, extracted domain crates, native/OCI extensions, MCP and manifest providers, content contributions, and frontend adapters. The archived `runtime-capability-declarations` change added stable authority-neutral declarations and diagnostics, but intentionally left construction, admission, dispatch, generation leases, and shutdown under legacy owners.

Current runtime authority remains split across `setup.rs`, `EventBus`, `loop.rs`, interactive coordination, ACP workers, daemon execution, session snapshots, command registries, and frontend-local state. Using this same broad integration path to repair Omegon couples recovery to the system under repair.

## Scope

- Produce and release-test a separately runnable maintenance executable before changing normal runtime authority.
- Define minimum durable session facts and one supervisor implementation instantiated once per session across interactive, ACP, Web/IPC, daemon, and bounded hosts.
- Evolve runtime capability declarations into a pre-activation contribution graph with deterministic dependencies, trust admission, quarantined probing, generation lifecycle, and typed degradation.
- Route model tools, operator actions, trust-boundary calls, durable mutations, and host effects through one admission and invocation-lease path.
- Make invocation dispatch crash-consistent and retry-safe through durable call states and owner-enforced deduplication semantics.
- Separate the default loop into a release-coupled policy driver that proposes transitions to the kernel session state machine.
- Build a complete semantic session event spine before migrating current snapshots, checkpoints, journals, or audit streams.
- Extract optional memory, lifecycle, planning, context, provider, tool, orchestration, content, and frontend domains only after the shared authorities exist.
- Establish contribution and artifact budgets plus release composition locks.
- Co-deliver durable architecture/developer documentation and applicable public site pages, shared snippets, migration guidance, and operator warnings within every implementation lane.

## Non-goals

- A Rust dynamic-library ABI or universal runtime service locator.
- Reimplementing DeepSeek Harness's Cordis framework.
- Treating every crate, service, or process as an operator-installable plugin.
- Hot-reloading the supervisor, admission combiner, session protocol, or loop during active turns.
- Treating manifests or capability declarations as sandbox enforcement.
- Moving code solely to reduce line counts or dependency-tree appearance.
- Replacing tool-specific validation, RBAC, secret guards, path boundaries, or operator approval with declaration metadata.
- Migrating optional domains before the maintenance and durable-authority prerequisites pass.

## Success criteria

- The maintenance executable starts and performs bounded diagnostic, denial/quarantine, stale-record-pruning, audit, and offline-verification operations without loading the normal TUI, default loop, project plugins, MCP, mutable packs, or optional lifecycle services.
- Every session has exactly one authoritative supervisor and recoverable prompt, queue, cancellation, invocation, and terminal state.
- Required contribution composition is deterministic and rejects unresolved, ambiguous, cyclic, untrusted, or unsupported graphs before promotion.
- No privileged invocation executes without a current capability owner, admission decision, generation-bound lease, and declared effects.
- Unsettled dispatched mutations recover as unknown completion and are never retried without owner-enforced idempotency or deduplication.
- TUI, ACP, Web, IPC, CLI, daemon, and headless projections cannot advertise or execute authority beyond the same runtime generation.
- Optional domains can be absent or fail independently without blocking the maintenance artifact or constitutional kernel.
- Normal and maintenance release artifacts carry verifiable required-module identities and pass their composition matrices.
- Every completed lane has reconciled its source design, OpenSpec artifacts, durable docs, and applicable public site/snippet output with implemented and packaged behavior.
