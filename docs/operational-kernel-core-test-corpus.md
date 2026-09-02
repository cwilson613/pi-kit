# Operational kernel, core, and addon test corpus

## Purpose

This corpus proves that Omegon can add runtime capability without creating a
second authority path. It covers the reduced kernel, signed first-party core
components, and operator-managed SDK addons. It also covers the full product as
the accumulated composition.

The machine-readable catalog is
`fixtures/operational-kernel-core-corpus-v1.json`. The structural validator is
`scripts/check_operational_kernel_core_corpus.py`. Scenario IDs are stable join
keys for OpenSpec requirements, tests, runtime evidence, and promotion gates.

## Terms

- **Kernel** means the constitutional runtime and its typed route, session,
  invocation, lifecycle, and host-effect authorities.
- **Core component** means a signed, release-coupled `core:*` product component.
  SDK metadata cannot grant this class.
- **Addon** means an operator-managed SDK extension. An addon can contribute
  capability after trust admission but does not become a product component.
- **Executor** means an automated test or bounded runtime command that produces
  evidence for one scenario.
- **Oracle** means the observable result that determines scenario success.
- **Promotion profile** means the scenario set that must pass before a boundary
  can claim a higher maturity level.

## Evidence model

One semantic claim can require evidence from several layers. A unit test does
not replace an installed-artifact test. A packaged artifact test does not replace
a state-machine fault test.

| Layer | Proves | Typical executor |
|---|---|---|
| Contract | Stable types, strict decoding, and compatibility | Rust fixture round trips |
| Policy | Inventory, trust, dependency, and budget declarations | Python mutation tests |
| State machine | Authority, ordering, generation, and rollback invariants | Pure Rust tests |
| Component | Protocol, process, cancellation, and cleanup behavior | Native conformance fixture |
| Artifact | Physical dependency and additive-composition boundaries | Composition ladder |
| Distribution | Installed layout, provenance, activation, and rollback | Channel runtime smoke |
| Platform | Native cleanup and target-specific behavior | CI or release target jobs |

Every catalog row has `implemented` or `planned` evidence. An implemented row
must name at least one existing executor path and command. A planned row remains
part of coverage calculations but cannot satisfy a promotion gate.

## Corpus dimensions

The corpus varies these dimensions independently:

1. Artifact: `kernel-only`, `kernel+core`, `task-capsule`, `full-product`, or
   `maintenance`.
2. Packaging class: constitutional resident, host service, signed core
   component, shipped content, or operator-managed SDK addon.
3. Execution provenance: scripted conformance, admitted provider, local owner,
   native RPC, remote transport, or distribution verifier.
4. Authority mode: sessionless conformance, authority-backed session,
   maintenance authority, or no runtime authority.
5. Surface: bounded, TUI, ACP, Web, IPC, CLI, daemon, model, or maintenance CLI.
6. Lifecycle phase: discovery through retirement and recovery.
7. Generation relation: initial, candidate, same-generation respawn,
   replacement, stale, retained previous, or retired.
8. Fault phase: input, trust, route, provider, invocation, publication, cleanup,
   distribution verification, switch, or replay.
9. Cleanup boundary: no resource, local process tree, local task/socket/writer,
   or remote best effort.
10. Distribution: source, linked, archive, installer, switch, Homebrew, Nix,
    OCI, or unsupported npm.

The sentinel matrix covers high-risk pairs. Platform jobs can generate more
pairwise rows under artifact and channel constraints. The default suite must not
generate a full Cartesian product.

## Invariant catalog

### Composition

- One composition generation is active.
- Publication is atomic across schemas, actions, routes, services, and caches.
- Candidate failure preserves the prior active generation.
- Runtime policy does not rewrite package composition.
- SDK metadata cannot mint `core:*` authority.
- Additive components do not change kernel host bytes or unrelated behavior.

### Session and route

- One supervisor owns session truth.
- Authority is durable before projection or dispatch.
- Terminal settlement occurs exactly once.
- A missed advisory event reconciles from the next authoritative snapshot.
- Every production request captures a route lease before dispatch.
- Retries, fallback, and context repair retain explicit request lineage.

### Invocation

- Preparation is durable before a lease is returned.
- Dispatch is durable before owner entry.
- Generation and policy are revalidated before execution.
- Unknown mutating work is not replayed automatically.
- Terminal settlement occurs exactly once.
- Authority can narrow but cannot widen downstream.

### Lifecycle

- Candidate declarations remain hidden until atomic publication.
- Active work retains its captured generation.
- Changed generations publish only at a quiescent boundary.
- Replacement publishes before the old generation retires.
- Stale handles and leases cannot enter an owner.
- Cleanup claims do not exceed observed evidence.

### Bounded execution

- The task manifest is admitted before session, route, tool, or process authority.
- The next governed action is checked before it starts.
- Token exhaustion prevents the next network request.
- Tool exhaustion prevents lease creation and owner entry.
- The result contains admitted and observed limits.
- Owned resources settle before the process returns its terminal result.

## Scenario families

### KRN: Reduced kernel

`KRN-001` proves deterministic scripted completion with no production route
claim. `KRN-002` runs the same turn before and after additive codescan
composition. `KRN-003` is the main acceptance: one provider-backed bounded turn
uses shared route, session, and terminal authorities. `KRN-004` cancels that turn
during an active provider request. `KRN-005` preserves the dependency,
capability, process, schema, and size ratchets.

`KRN-003` and `KRN-004` execute through `omegon-kernel-runtime`. Their loopback
fixture verifies the endpoint-bound bearer credential, admitted native model,
durable route lease before network dispatch, structured completion, and exactly
one terminal turn fact. The cancellation scenario waits for the active request,
sends `SIGINT`, and requires both local transport settlement and a durable
`cancelled` outcome. A separate deadline case requires the same guarantees with
a `timed_out` outcome. The provider-backed profile also requires the implemented
budget, authority-failure, and cleanup executors; no adjacent scenario can
substitute for them.

The provider fixture must be local and deterministic. Live upstream tests are
supplemental. They must not be required for the default acceptance gate.

### BND: Prospective bounds

`BND-001` rejects an invalid task manifest before authority starts. `BND-002`
refuses the next provider request when it would exceed the token budget.
`BND-003` refuses the next tool call before lease creation when the tool budget
is exhausted.

The reduced-kernel black-box fixture rejects an unknown task field with a
structured zero-turn error before route loading or authority creation. The
installed `task-capsule` acceptance also rejects an unknown field and verifies
that no workspace or user runtime state appears before refusal.

`BND-002` uses a provider response that requires continuation and reports its
token usage. Before the continuation request, the reduced kernel compares
cumulative observed usage and the known next input cost with the admitted token
budget. Exhaustion creates no second route lease or network request, closes the
turn once, and returns exit code 2 with admitted and observed token evidence.

`BND-003` runs against a real native-extension process. The shared kernel
runtime reserves the prospective tool count and constructs the invocation lease
as one operation. The exact-boundary call reaches the owner. The next call
returns typed exhaustion before lease construction or RPC dispatch. Extension
initialization traffic is permitted, but a fresh owner-entry marker proves that
the exhausted `execute_tool` request never enters the component.

`BND-004` is the production integration sentinel. A bounded task manifest admits
`tool_budget` before authority starts, and a model-originated native tool call
consumes that shared budget. The exact-boundary call enters its owner. The next
call creates no invocation preparation, lease, or RPC and returns structured
exhaustion after session and process authority settle. `BND-003` cannot substitute
for this row because it exercises the authority contract directly rather than a
manifest-driven bounded turn.

The low-level boundary matrix covers one below, exactly at, and one above the
wall deadline, turn limit, token limit, and tool limit. The turn, token, and tool
process fixtures also prove that the first refused action creates no route lease,
invocation lease, network request, or native owner entry. The timeout fixture
uses a bounded local provider and proves transport plus authority settlement.

### CMP: Signed core components

`CMP-001` proves typed kernel absence without fallback discovery. `CMP-002`
proves additive restoration with unchanged host bytes. `CMP-003` is the generic
promotion gate for a new `core:*` component.

Every promoted core component must provide portable contracts and signed
identity. It must also prove kernel absence, additive restoration, full-product
retention, policy disablement, protocol mismatch, lifecycle cleanup, aggregate
budgets, exact package inventory, and SDK self-promotion refusal.

### ADD: Operator-managed addons

`ADD-001` proves that an SDK addon cannot claim kernel-release or `core:*`
authority. Addon tests reuse the native-extension conformance process for
handshake, readiness, invocation, cancellation, crash, replacement, and cleanup.
Remote transports must report cleanup as best effort or unverified when the host
cannot observe remote settlement.

### LIF: Generation lifecycle

`LIF-001` defers changed-generation publication during active work. `LIF-002`
proves failed publication leaves the prior generation callable and hides all
candidate declarations. `LIF-003` denies stale handles and leases after a
successful replacement.

The lifecycle corpus distinguishes same-generation respawn from changed-source
replacement. Restart budgets are generation-local. Source changes require a new
candidate and cannot inherit execution authority from the old generation.

Native extension generation IDs bind to the admitted source digest. The
publication coordinator retains one hidden pending generation per contribution;
accepting C first settles B's process resources without changing active A.
Publication is an explicit transaction that requires the session supervisor to
be idle, its queues and durable invocation authority to be settled, and direct
extension calls to be quiescent. Turn closure and the next turn start do not call
that transaction. An idle `/runtime refresh` or extension refresh alias stages
admitted installed bytes and invokes the transaction. Same-generation
`/runtime replace <name>` retains its admitted-snapshot contract. Commit replaces
the EventBus graph and published digest fence,
then retires A under bounded process ownership. EventBus leases and direct
polling handles enter through the same generation fence, so retained A authority
fails before native RPC while fresh admission resolves B. Remote peers remain
best-effort or unverified unless their own protocol acknowledges settlement.

### AUT and SUR: Authority and surfaces

`AUT-001` injects authority-write failures before projection and dispatch.
`SUR-001` feeds one canonical snapshot and action set to TUI, ACP, Web, IPC, CLI,
and daemon adapters. `SUR-002` makes each applicable adapter miss terminal advice
and then reconcile from an authoritative snapshot.

Surface comparison ignores transport framing, required redaction, and declared
unsupported bindings. It compares identity, queue, active turn, terminal state,
generation, action availability, owner, denial reason, and lifecycle health.

The activity revision is the durable session-authority sequence, not a
surface-local counter. Caches compare revisions only within the same session,
authority stream, runtime generation, and composition generation. A newer
revision replaces queue, active-turn, and terminal state atomically; an older or
unversioned observation cannot override it. Equal revisions are idempotent only
when their complete semantics match, and a lineage change requires explicit
session replacement. Persistent TUI, ACP, Web, IPC, and daemon adapters reconcile
through this contract. One-shot CLI output carries the same representable
identity, terminal state, actions, owners, and denials, but explicitly reports
persistent busy reconciliation as unsupported.

### DST: Distribution trust

`DST-001` rejects direct installation when authenticity evidence is absent or
invalid even if the checksum matches. `DST-002` proves version switching cannot
publish a mixed host/component generation. `DST-003` proves Nix remains an
authenticated host-only composition. `DST-004` requires digest-bound OCI
signature, SBOM, provenance, and composition identity.

`DST-001` uses two executors. The real maintenance verifier authenticates the
signed archive identity and exact extracted member tree. The installer campaign
proves this external verification precedes extraction, exact-tree revalidation
precedes other candidate execution, malformed success output fails closed, and
every refusal preserves the active selector without retained staging. An
existing version directory is never substituted for the authenticated bytes.

`DST-002` holds the release selector lock while it captures the active
generation's maintenance executable. Only that captured executable verifies the
canonical archive evidence and extracted tree. One `current` replacement selects
the host, maintenance companion, components, content, locks, and receipt. Refusal
removes switch-owned work and preserves the prior callable generation.

`DST-004` is a policy and CI verifier. Its deterministic evidence record requires
the signature, SBOM, provenance, composition identity, and explicit composition
class to bind one immutable image digest. It rejects publication and live-registry
claims. Passing this row does not claim that a production image or attestation was
published.

Archive mutation tests must cover missing, duplicate, misplaced, unexpected,
case-colliding, traversing, linked, oversized, wrong-target, wrong-digest, and
wrong-identity members.

### CLN: Cleanup

`CLN-001` proves timeout and cancellation terminate the complete owned native
process tree. `CLN-002` proves remote cleanup claims remain honest when remote
state cannot be observed.

Cleanup injection points include before spawn, after spawn, readiness, active
provider request, active tool call, drain, graceful stop, escalation, reap, and
durable-writer settlement. A cleanup timeout is degraded or unverified evidence.
It is not success.

## Promotion profiles

### Scripted kernel baseline

This profile can pass before provider extraction. It proves deterministic kernel
loop behavior, additive noninterference, and the physical dependency boundary.
It cannot claim production bounded execution.

### Provider-backed kernel acceptance

This profile requires `KRN-003`, `KRN-004`, `BND-001`, `BND-002`, `BND-003`,
`BND-004`, `AUT-001`, and `CLN-001`. It proves a real bounded turn through shared
authority, including task-manifest tool admission through the native invocation
path.

### Signed core component promotion

This profile requires kernel absence, additive restoration, generic core
qualification, lifecycle deferral and rollback, stale-generation denial,
distribution composition evidence, and local cleanup.

### SDK addon promotion

This profile requires trust non-promotion, lifecycle rollback and stale denial,
surface narrowing, and cleanup evidence appropriate to the transport. It does
not require release package membership or signed `core:*` identity.

### Milestone PR readiness

This profile contains every sentinel scenario in the catalog. It passes only
when every row has implemented evidence and all referenced executors pass. The
OpenSpec task list, documentation gates, broad tests, Clippy, and archive-check
remain separate required evidence.

## Remaining promotion and gate wiring

Every corpus row now names implemented evidence. Profile checks still need to run
in their owning CI and release lanes; structural corpus completeness does not
substitute for executing the referenced commands.

1. Require signed-core and SDK-addon profiles in their owning gates. Run milestone
   readiness only after distribution, documentation, platform, and repository
   landing gates pass.

Do not implement these lanes by adding policy to native transport, renderer, or
candidate code. Bootstrap verification belongs before extraction, publication
belongs to the supervisor coordinator, generation fencing belongs before RPC,
and semantic reconciliation belongs in the shared activity projection.

## Failure reporting

Each executor should report:

- scenario ID.
- artifact and installation identity.
- composition and contribution generations.
- route and invocation evidence when applicable.
- admitted and observed budgets.
- fault injection point.
- terminal outcome and exit status.
- process and cleanup inventory.
- evidence layer and platform.
- bounded diagnostic code.

Logs and snapshots are evidence. They do not grant authority. Diagnostics must
not contain credentials, secret values, restricted provider continuity, or
unbounded child output.

## Adding a component or addon

1. Add or update the first-party domain classification.
2. Select the signed-core or SDK-addon promotion profile.
3. Add component-specific scenarios only for behavior not covered by sentinels.
4. Bind each required scenario to the narrowest deterministic executor.
5. Add a real-process row for process-backed behavior.
6. Add kernel absence and additive restoration for a signed core component.
7. Add installed-distribution evidence for release-coupled bytes.
8. Run the profile gate. Do not mark planned evidence as implemented.

Do not copy the loop, route service, lifecycle owner, or invocation authority
into a component fixture. The test must fail if integration creates a parallel
authority path.
