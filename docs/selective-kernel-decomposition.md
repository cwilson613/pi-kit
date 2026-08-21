+++
id = "fd735ff8-4f53-45d4-92cf-099b8ef2fcde"
kind = "document"
title = "Selective Omegon kernel decomposition"
status = "decided"
tags = ["architecture", "kernel", "plugins", "decomposition", "runtime"]
aliases = ["omegon-kernel-decomposition", "selective-plugin-decomposition"]
imported_reference = false

[publication]
enabled = false
visibility = "private"

[data]
dependencies = ["binary-composition-and-kernel-admission", "harness-architecture-parity"]
open_questions = []
related = ["coding-harness-philosophies"]
+++

# Selective Omegon kernel decomposition

## Decision

Omegon will continue converging on a plugin architecture, with one important
qualification:

> Everything optional belongs behind a typed, inspectable, lifecycle-owned
> contract. Not everything is an ordinary or dynamically reloadable plugin.

The target is a small constitutional kernel surrounded by selectively composed
system modules, in-process services, out-of-process contributions, content
packs, and frontend adapters. The kernel owns the authorities whose duplication
can violate operator agency, security, provenance, replay truth, or process
cleanup. Product behavior outside that boundary should be replaceable or absent
without destabilizing the kernel.

This refines, rather than replaces, the capability-admission design in
[`binary-composition-and-kernel-admission.md`](binary-composition-and-kernel-admission.md).
That design already established the essential laws: composition is not
admission, visibility is not authority, one decision feeds many projections,
execution rechecks authority, and transitions are generation-scoped.

Where that document groups the loop and minimal tools with `omegon-kernel`, this
assessment separates artifact residency from constitutional authority. Release
packages may co-ship both tiers, but the minimal kernel artifact contains only
constitutional authorities; loop and tool implementations remain
release-coupled system modules.

## Why now

The following observations record the pre-Slice-2 baseline: Omegon had several
contribution and modularity mechanisms, but no single composition authority:

- statically linked `Feature` implementations behind `EventBus`;
- extracted domain crates behind main-crate adapters;
- native and OCI extensions over JSON-RPC;
- MCP, HTTP, script, OpenAPI, and manifest-backed tool providers;
- skills, prompts, personas, and workflows as content contributions;
- a growing semantic-surface layer used most consistently by TUI and ACP, while
  Web and IPC retain parallel projection/event paths;
- authority-neutral runtime-capability vocabulary and a post-finalization
  diagnostic inventory, currently exercised only by tests and not used for
  construction, admission, or dispatch.

At the baseline, the result was extensible but not yet selectively decomposable. `setup.rs`
constructs concrete product features procedurally. `EventBus` owns features but
also hard-codes tool classes, profile filtering, timeout exceptions, and
first-registration-wins collision behavior. `loop.rs` combines turn sequencing,
provider streaming, admission, tool scheduling, context policy, compaction,
behavior nudging, plan reconciliation, and concrete feature requests. Frontends
and daemon paths retain partially independent command and runtime authorities.

Current admission is layered but incomplete. Secret guards, Styrene role checks,
configured permission rules, host approval, and path-boundary retry are combined
inside tool dispatch. Unconfigured and unknown tool names currently default to
allow in permission policy and RBAC mapping, and subject extraction recognizes
selected tool names. The fail-closed effect model below is a target invariant,
not current behavior.

The opportunity is not to invent another plugin API. It is to make the existing
contracts authoritative and remove concrete policy from around them.

## DeepSeek Harness: what "everything is a plugin" means

The reference baseline is first-party DeepSeek Harness at commit
[`99f6f02f`](https://github.com/deepseek-ai/deepseek-harness/commit/99f6f02fecdb7dff40c3fbc9470f5907c29f74ca),
tag `dsh-v0.1.0-rc.7`, observed 2026-08-17.

The full evidence profile lives in
[DeepSeek Harness architecture profile](harness-architecture-parity/deepseek-harness.md),
and the cross-harness synthesis lives in
[Coding harness philosophies and tradeoffs](harness-architecture-parity/philosophies.md).
This section retains only the conclusions needed for the Omegon decision.

Its architecture document states that the model adapter, tool registry, session
log, and agent loop are Cordis plugins. The useful part of that claim is not
source-file modularity. It is a set of runtime laws:

1. A context resolves stable service contracts rather than concrete providers.
2. Dependencies are declared and activation waits for them.
3. Registrations are effects owned by one plugin lifecycle and unwind on unload.
4. Events declare one of four dispatch contracts: synchronous fire-and-forget
   `emit`, awaited first-bail `serial`, awaited concurrent `parallel`, or
   short-circuitable around-middleware `waterfall`.
5. Profiles compose a process and bundles distribute ordered patch layers.
   Presets define model-facing agent composition; a preset generation is mounted
   once and shared by agents that join it, while scoped lookup controls which
   registrations each agent sees.
6. Service implementation scope and model-visible registration scope are
   distinct concerns.
7. Durable session events are separate from live interception events.
8. "Model-visible means logged": every model request can be reconstructed from
   the append-only session log. In the shipped base composition, checkpoint
   policy additionally flushes the complete request prefix before dispatch.
9. Startup fails loudly for unresolved or failed required entries and disposes
   the partial Cordis tree. Agent setup is rollback-covered before publication.
   Both guarantees depend on resources being correctly effect-owned; they are
   not transactions over arbitrary external side effects.
10. A selected loop implementation still owns coherent turn sequencing even
    though that implementation is replaceable at a composition boundary.

Primary evidence:

- [DeepSeek Harness architecture](https://github.com/deepseek-ai/deepseek-harness/blob/99f6f02fecdb7dff40c3fbc9470f5907c29f74ca/docs/architecture.md)
- [Cordis primer](https://github.com/deepseek-ai/deepseek-harness/blob/99f6f02fecdb7dff40c3fbc9470f5907c29f74ca/docs/cordis-primer.md)
- [capability seams](https://github.com/deepseek-ai/deepseek-harness/blob/99f6f02fecdb7dff40c3fbc9470f5907c29f74ca/docs/capability-seams.md)
- [scope](https://github.com/deepseek-ai/deepseek-harness/blob/99f6f02fecdb7dff40c3fbc9470f5907c29f74ca/docs/subsystems/scope.md)
- [sessions](https://github.com/deepseek-ai/deepseek-harness/blob/99f6f02fecdb7dff40c3fbc9470f5907c29f74ca/docs/subsystems/session.md)
- [tools](https://github.com/deepseek-ai/deepseek-harness/blob/99f6f02fecdb7dff40c3fbc9470f5907c29f74ca/docs/subsystems/tools.md)

### The non-plugin DeepSeek host substrate

DeepSeek still depends on non-plugin host machinery. Its CLI creates the root
Cordis context, installs the loader, establishes launch-environment and signal
handling, and owns root teardown. Cordis defines service resolution, fibers,
effects, events, isolation, and dependency activation. Above that host layer,
session vocabulary, persistence, and the concrete loop are plugin-owned
contracts rather than privileged substrate. A selected loop provider still owns
coherent sequencing while mounted.

The transferable lesson is therefore:

> A plugin architecture moves product capabilities behind replaceable seams; it
> does not remove the need for a small substrate that defines identity,
> authority, lifecycle, durability, and failure semantics.

### Mechanisms not to copy directly

Omegon should not reproduce Cordis as a universal dynamic service locator.
Specifically:

- avoid importing Cordis-style runtime service-name resolution as Omegon's
  universal dependency mechanism when Rust traits and typed handles can express
  the contract;
- avoid around-middleware waterfalls as the default policy primitive because a
  listener can accidentally suppress the underlying operation;
- avoid automatic cascading unload/reload for stateful services during active
  work;
- avoid executable configuration expressions;
- avoid same-process trust for operator-installed third-party code;
- avoid package fragmentation that does not create an ownership or failure
  boundary;
- avoid full copied agent presets that silently drift from their source; prefer
  overlays or explicit inheritance where lifecycle semantics permit;
- avoid a user-visible `PENDING` state with no bounded startup diagnosis.

## Omegon's existing convergence

### `Feature` and `EventBus`

`omegon-traits::Feature` already combines tools, commands, context injection,
event handling, and session-lifecycle participation through
`BusEvent::SessionStart` and `BusEvent::SessionEnd`. `EventBus` owns feature
instances and provides a directional interaction model:

```text
runtime -> BusEvent -> Feature
runtime <- BusRequest <- Feature
```

This is a useful in-process contribution contract because it prevents callbacks
from coupling shared traits back into the binary. It is not yet the target
kernel registry because `EventBus` still owns concrete tool policy and execution
exceptions. Evidence:

- `core/crates/omegon-traits/src/lib.rs`
- `core/crates/omegon/src/bus.rs`
- `core/crates/omegon/src/capability_admission.rs`

### Runtime capability declarations

At the pre-Slice-2 baseline, `RuntimeCapabilityId`, capability kinds, owner
vocabulary, invocation bindings, groups, and diagnostics were only the seed of
a future composition graph. The read-only capability inventory did not control
construction or dispatch, and duplicate owners were resolved before projection.

Slices 2.1 through 2.7 now freeze full contribution declarations, validate one
deterministic candidate graph, and atomically derive EventBus feature publication
and legacy compatibility caches from the accepted graph. Dynamic code requires
source-bound trust or verified-confinement evidence before probe execution;
readiness, rollback, restart quarantine, composition generations, and cleanup
assurance are typed. Native and ACP `/status` consume one semantic composition
projection. Privileged invocation leases and dispatch authority remain Slice 3,
so the authority-neutral capability inventory is still not itself an execution
grant.

### Extracted domains

Memory, lifecycle, Git, secrets, RBAC, skills, web extraction, and work models
already have crate boundaries. Those crates are domain engines or contracts,
not automatically plugins. Their main-crate adapters still decide startup,
provider access, context injection, tools, and surface behavior.

The decomposition target is to make those adapters declare requirements and
contributions to a common runtime graph, while keeping domain implementation in
the owning crate.

### Out-of-process contributions

Native/OCI extensions and MCP demonstrate that contributions can live outside
the process and be adapted into `Feature` implementations. Their transport
adapters remain distinct, but now publish lifecycle and transition policy into
one authoritative composition graph. Extension and MCP readiness and cleanup
are bounded; extensions use generation-local restart quarantine, while MCP,
Armory, and HTTP retain transport-specific best-effort cleanup. Process and
container boundaries remain crash-isolation opportunities, not proof of
confinement.

### Semantic surfaces

Shared projections under `core/crates/omegon/src/surfaces/` are moving
presentation away from runtime policy, especially for TUI and ACP. Web still
owns substantial Web-specific projection state, and IPC primarily transports
runtime events and protocol DTOs. This is a partially established frontend
boundary, not a universal one. A semantic surface becomes genuinely composable
only when it consumes shared contracts or domain read models rather than
concrete `features::*` types.

## Constitutional kernel

The kernel contains only behavior whose duplication or replacement can break a
system-wide invariant. Internal crate boundaries are encouraged, but these
authorities are not ordinary operator-installed plugins.

### 1. Identity and protocol

The kernel owns stable IDs for runtime, contribution, capability, session,
prompt, turn, invocation, lease, and generation. It owns renderer-neutral event,
command, admission, and terminal-outcome vocabulary. Serialized compatibility
is explicit and versioned.

### 2. Contribution graph and lifecycle

The kernel owns declaration registration, deterministic dependency validation,
collision handling, generation construction, activation, quiescence, promotion,
rollback, and shutdown.

Dynamic protocols use two phases. A static manifest preflight declares identity,
protocol range, minimum dependencies, requested trust, and probe requirements.
Before executing probe code, the host requires either explicit trusted-code
admission or verified confinement; a host-effect lease alone cannot contain an
unsandboxed process. The host may then start the contribution in quarantine,
with no brokered host-effect lease, to negotiate its frozen declaration set.
Only after graph validation, readiness, and capability admission may the
candidate generation be atomically promoted.
Heartbeat loss, startup timeout, crash loops, dependency degradation, drain,
shutdown deadlines, forced cleanup, and quarantine are host-owned lifecycle
states.

The versioned preflight binds that request to immutable source bytes by digest
and identifies whether evaluation, initialization, capability discovery,
context generation, or connection would execute. Admission evidence is a
separate host-produced object bound to the same contribution and source digest.
Trusted-code evidence names kernel-release or operator-policy authority.
Confinement evidence is valid only for a host-verified OS or OCI boundary that
blocks direct filesystem, process, network, and secret access and permits
privileged effects only through brokers. Existing installation, enablement,
maintenance allow, trusted-directory, and manifest-request state cannot produce
this evidence.

The graph must reject:

- duplicate capability ownership without an explicit replacement declaration;
- ambiguous invocation names;
- dependency cycles;
- missing required services;
- unsupported protocol ranges;
- contributions that request undeclared effects.

### 3. Session and turn supervision

The kernel defines one supervisor contract and implementation, instantiated once
per session. TUI, ACP, Web, IPC, daemon, and headless ingress submit to the
owning session supervisor rather than maintaining competing prompt, busy,
cancellation, or completion authorities. Cross-session hosts may manage many
supervisor instances, but each session has exactly one authoritative queue and
active-turn state.

The replaceable loop is a policy driver. It proposes typed step, message,
invocation, continuation, and terminal transition intents. The kernel session
state machine validates and durably commits each transition exactly once before
publishing snapshots. The loop does not independently mutate canonical session
truth or publish terminal completion.

### 4. Invocation pipeline

Model tools, operator actions, trust-boundary crossings, calls consuming caller
authority, durable mutations, and host-effect-bearing internal calls execute
through one kernel path:

```text
resolve capability and owner generation
  -> evaluate current admission
  -> persist Prepared and issue an execution lease
  -> persist Dispatched
  -> hand the request to the in-process or RPC owner
  -> persist owner acknowledgement
  -> record progress and persist terminal settlement
  -> close the lease exactly once
```

The pipeline does not branch on names such as `bash`, `read`, or
`memory_store`. Parallel safety, timeout class, retry class, idempotency,
transaction behavior, and required host effects are declaration metadata.
Pure in-process computation and read-only domain queries may use typed service
handles directly; they do not become universal kernel RPC.

Invocation durability follows this state machine:

```text
Prepared -> Dispatched -> Acknowledged -> Settled
            -> Unknown
```

`Prepared` is durable before authority is leased; `Dispatched` is durable before
transport handoff. A stable call ID and deduplication key cross every RPC
boundary. Every unsettled `Dispatched` call is conservatively unknown completion
after recovery. It is not retried unless the owner contract proves idempotency
or deduplication. If result or audit settlement cannot be persisted, the kernel
fences further mutation, writes an emergency recovery record through its
last-resort channel, and does not report ordinary completion.

Every mutating execution declaration identifies a durable domain and fence key.
Emergency fence evidence is append-only and independent of the authority stream
whose failure triggered it; it binds the invocation, visible call, capability,
owner and composition generations, lease, session, turn, and failure phase.
Matching mutation admission checks this shared evidence immediately before
preparation. Malformed evidence and emergency-writer failure fail closed. Normal
execution cannot clear a fence; only deterministic reconciliation or an
explicit audited operator recovery decision may do so.

Stable call identity is also checked against unknown invocations across prior
turns. A mutating unknown cannot be replayed unless its original persisted
contract was idempotent or used owner-enforced deduplication for that exact call
ID; replacement metadata cannot retroactively grant safety. Legacy unknown
records fail closed. This denial does not itself enable safe replay: attempt
lineage, request fingerprints, and a retry scheduler remain separate work.

### 5. Admission combiner and host effects

Policy providers may be replaceable, but the kernel combines and enforces their
decisions monotonically. It owns execution leases and privileged host-effect
entry points. Plugin-declared metadata requests authority; it does not prove
confinement or grant authority.

Unknown owners, capabilities, effects, schemas, or provenance fail closed.
Filesystem mutation, process execution, secret delivery, network egress,
package installation, resource opening, and terminal creation must carry caller
identity, generation, scope, and audit context.

### 6. Provider route lease

Provider clients and catalogs are replaceable services. The kernel owns the
selected route lease for each inference request and records provider identity,
model identity, schema dialect, credential source class, fallback reason, and
generation with the turn. A provider contribution cannot silently broaden
fallback or substitute a model family.

### 7. Durable semantic event contract

The kernel owns append-only semantic session event identity, ordering, and
terminal closure. Storage backends and model projections may be replaceable.
Durable facts must be sufficient to recover:

- admitted operator input;
- model-visible context and tool schemas by reference or canonical snapshot;
- assistant stream and committed message;
- tool calls, progress, and terminal results;
- turn/step boundaries and terminal status;
- route and capability generations;
- cancellation and interruption evidence.

This adopts DeepSeek's strongest invariant without requiring its implementation:
model-visible input must be explainable from durable evidence.

This contract is distinct from the current `SessionLog` feature and
`.omegon/agent-journal.md`; that journal is a human-oriented narrative summary,
not replay authority. Current persistence is plural: resumable sessions are
atomic snapshots of the LLM-facing view, checkpoints are append-only metadata,
and journal/audit streams have separate purposes and schemas.

### 8. Process and resource ownership

Every spawned process, task, socket, listener, subscription, temporary file, and
durable writer has one host-recorded owner and generation. For process trees the
host can own, cancellation, timeout, failed startup, replacement, and shutdown
settle the complete tree before reporting terminal state. Cross-boundary
processes, including Windows-host executables launched from WSL, are recorded as
degraded or unverified rather than falsely settled; profiles requiring strict
cleanup reject those transports.

### 9. Recovery substrate

The constitutional kernel retains only the recovery substrate required to start
and explain failure:

- signed composition verification and minimal configuration parsing;
- contribution identity, disable markers, and graph diagnostics;
- protocol and dependency diagnostics;
- permission, path, secret-redaction, and audit enforcement;
- process cleanup;
- machine-readable session/event framing diagnostics.

Slice zero provides only the bounded diagnostic, deny/quarantine,
stale-record-pruning, audit, and offline-verification commands defined in
[`omegon-maintain.md`](omegon-maintain.md). Generic coding tools, configuration
repair, semantic session repair, package/update logic, and rollback are not
part of this slice or implied by later authority work; adding them requires
separate requirements and tasks. The companion executable must not load the
normal TUI, default loop, project configuration, project plugins, mutable packs,
MCP servers, or optional lifecycle systems during startup.

## Contribution tiers

"Plugin" is an architectural role, not one trust or deployment mechanism.

| Tier | Examples | Replacement boundary | Trust and lifecycle |
|---|---|---|---|
| Constitutional kernel | IDs, contribution graph, session state machine/supervisor, admission combiner, invocation leases, event sequencing, recovery substrate | Release only | Static, fail-closed, always available |
| System module | Maintenance workflows resident only in the companion `omegon-maintain` executable, default loop driver, provider route service, session projection, core host-effect executors | Boot or quiescent generation boundary | In-tree, release-coupled, privileged contract |
| In-process service | Memory, lifecycle, Git, context/compaction, behavior policy, codescan, work aggregation | Boot; later generation replacement only where proven safe | Statically linked Rust, explicit dependencies and teardown |
| Out-of-process contribution | Native/OCI extensions, MCP, HTTP/OpenAPI, optional provider/tool adapters | Supervised process/protocol generation | Unconfined processes are trusted host-authority code; least authority requires verified OCI/OS confinement and brokered host effects |
| Content pack | Skills, prompts, personas, tones, workflows, catalogs | Admission/projection generation | Data by default; executable assets require explicit trust |
| Frontend/host adapter | TUI, ACP, Web, IPC, CLI, daemon ingress, schedulers | Client connection or host boot | No independent runtime authority |

System modules are replaceable architecture without being ordinary plugins.
They can use the same declaration and lifecycle vocabulary while remaining
release-coupled and unavailable to arbitrary third-party replacement.

An unsandboxed native, script, or MCP process runs with the operator's host
authority regardless of its manifest. It must be admitted as trusted code.
"Least authority" applies only when a verified OCI or OS boundary blocks direct
filesystem, process, secret, and network access and forces privileged effects
through kernel brokers. Requested confinement that cannot be established fails
closed rather than degrading to an unconfined process.

## Three separate composition planes

Omegon should preserve DeepSeek's distinction between process composition and
agent composition while adding an explicit host plane.

### Product profile

Selects process-resident modules and required contributions, for example:

- `maintenance` companion-artifact profile: the compiled narrow diagnostic,
  denial/quarantine, stale-record-pruning, audit, and offline-verification
  profile of the separate `omegon-maintain` executable, not a mode of `omegon`;
- `interactive`: kernel plus TUI, normal provider/services, and optional
  operator bindings to the companion maintenance executable;
- `headless`: kernel, bounded host, selected services, no TUI;
- `daemon`: kernel, long-running host adapters, scheduler/triggers;
- `full`: interactive plus lifecycle, memory, orchestration, and rich packs.

A profile determines residency and requirements, not callability.

### Agent/session preset

Selects the capabilities and policy defaults visible to one session or child:

- tool and skill subsets;
- provider/model policy;
- context and compaction policy;
- memory/lifecycle participation;
- sandbox and host-effect restrictions;
- child/delegation limits.

An existing session retains its admitted generation or crosses a declared
quiescent migration boundary. It does not silently inherit a mutated preset.

### Frontend/host binding

Binds canonical runtime actions and projections to TUI, ACP, Web, IPC, CLI,
daemon ingress, or a scheduler. A binding can expose less than the admitted
runtime, never more. Transport availability is not capability authority.

## Service seams versus event seams

DeepSeek's separation is useful if Omegon makes dispatch semantics explicit.

| Need | Omegon contract |
|---|---|
| Direct capability call with one owner | Typed service trait/handle |
| Durable fact needed for recovery | Append-only semantic session event |
| Live observation with no authority | Best-effort notification derived from state |
| Ordered policy contribution | Typed decision list combined by kernel law |
| First-answer provider selection | Explicit registry with deterministic priority and diagnostics |
| Around behavior | Named middleware chain only where replacement is intentional and `next`/delegation cannot be omitted accidentally |
| Frontend recovery | Versioned snapshot plus monotonic cursor, never broadcast alone |

`BusEvent` and `AgentEvent` should not remain competing truth systems. The target
is durable facts and authoritative snapshots at the center, with feature and
frontend event adapters derived from them.

## Target graph

The following names describe logical ownership layers. Concrete package
extraction and names are deferred unless separately specified.

```text
omegon-kernel-contracts
  IDs, contribution declarations, semantic events, admission/lease DTOs,
  command/tool contracts, route identity, snapshots

omegon-kernel-runtime
  dependency graph, generation lifecycle, supervisor, invocation pipeline,
  admission combiner, event sequencing, recovery substrate

system modules
  maintenance workflows resident only in omegon-maintain, default loop driver,
  provider routing, session/model projections,
  host filesystem/process/network/secret effect executors

in-process services
  context/compaction, behavior, memory, lifecycle, Git, plans/work,
  codescan, provider transports, catalog registry/resolver

out-of-process contributions
  native/OCI RPC, MCP, HTTP/OpenAPI, remote providers and tools

content packs
  skills, prompts, personas, tones, workflows, catalog data

frontend and host adapters
  TUI, ACP, Web, IPC, CLI, daemon ingress, schedulers
```

Crate extraction is subordinate to this graph. Moving code without changing
authority, dependency, lifecycle, or failure isolation is not decomposition.

## Selective decomposition map

| Current subsystem | Target tier | First boundary to establish |
|---|---|---|
| `RuntimeCapabilityRegistry` | Kernel | Declarations become pre-activation input and dispatch authority rather than a read model |
| `EventBus` | Kernel registry adapter, then service/event adapter | Remove hard-coded tool classes, timeout names, disabled-name authority, and collision-by-order |
| Interactive coordinator/runtime supervisor | Kernel | Compile one frontend-neutral implementation and instantiate it once per session across hosts |
| `loop.rs` | System loop driver plus kernel invocation client | Extract admission, tool scheduling, host effects, compaction, and feature-specific requests |
| Conversation/session | Kernel event contract plus replaceable projections/storage | Inventory whole-file LLM-view snapshots, metadata checkpoints, narrative journal, and audit log; define semantic events before migration |
| Provider routing | System service | One provider declaration owns identity, auth class, inventory, dialect, bridge factory, and fallback compatibility |
| Core tools | System/in-process services | Replace name switches with declared effect and execution metadata |
| Permissions/RBAC/secrets | Policy providers plus kernel combiner/effect executors | Deny unknown effects and bind decisions to owner/generation |
| Memory | In-process service | Remove concrete `memory_store` knowledge from loop and provider resolution from feature |
| Lifecycle/OpenSpec/design | In-process service | Expose read/mutate/projection contracts without kernel or surface imports of concrete feature types |
| Plans/Workbench/work runtime | In-process aggregation plus semantic projection | Separate session-local plan authority, lifecycle artifacts, and Workbench read model |
| Native/OCI extensions | Out-of-process contribution | Negotiate typed capabilities beyond tools; generation-bind registrations and calls |
| MCP/manifest plugins | Out-of-process contribution | Join common discovery, admission, process ownership, and projection contracts |
| Skills/prompts/personas | Content pack | Loading requests admission; content cannot persist trust grants itself |
| TUI/ACP/Web/IPC | Frontend adapters | Consume one snapshot/action registry and remove local execution policy |
| Daemon/sentry/triggers | Host adapters/services | Submit through the same session supervisor and invocation authority |

## Decomposition sequence

The sequence deliberately establishes authority before moving optional code.

### Slice 0: baseline and maintenance profile

- Produce a separately runnable, release-coupled maintenance executable rather
  than a mode that depends on normal integration-binary startup.
- Implement it as workspace package `omegon-maintain` at
  `core/crates/omegon-maintain/`, with no dependency on package `omegon`.
- Define its narrow `maintenance` profile and command contract in
  [`omegon-maintain.md`](omegon-maintain.md).
- Inventory its transitive dependencies, startup tasks, tools, commands, and
  model-visible schema.
- Add a boot test with project plugins, MCP, lifecycle, memory, TUI, and mutable
  packs disabled.
- Package and launch-test it through source, linked development, and release
  artifact paths from this first slice.
- Record current session, command, and runtime recovery gaps.

Exit gate: an independently launchable, documented, tested fallback artifact
exists before extraction can make the normal product less reliable.

### Slice 1: minimum durable session authority

- Use the approved v1 facts `session.created`, `prompt.admitted`,
  `prompt.rejected`, `prompt.removed`, `turn.started`,
  `turn.interruption_requested`, `invocation.registered`,
  `invocation.classified_unknown`, `invocation.settled`, and `turn.closed`.
- Apply the strict contiguous ordering, idempotency, compatibility,
  deterministic recovery, and snapshot rules in
  [`runtime-session-semantic-protocol.md`](runtime-session-semantic-protocol.md)
  before publishing corresponding live snapshots.
- Persist a separate adjacent authority stream and reducer cache. Existing
  conversation snapshots, metadata checkpoints, journals, and audit streams
  remain compatibility projections rather than historical semantic truth.
- Refactor the existing uncompiled supervisor/prompt/turn extraction scaffold
  into one frontend-neutral implementation compiled into the runtime without
  losing the included coordinator's stale-interrupt and exactly-once settlement
  protections.
- Instantiate it once per session and route interactive, ACP, daemon, Web/IPC,
  and bounded ingress through it where semantics overlap.
- Remove included coordinator duplicates only after parity tests pass.
- Keep frontend busy/streaming state as a projection of the owning supervisor's
  durable state and snapshots.

Exit gate: completion, cancellation, queue state, and second-turn admission have
one recoverable authority per session across hosts.

### Slice 2: composition-authoritative contribution graph

- Extend capability declarations with dependencies, conflicts, owner tier,
  lifecycle, effect, timeout, retry, idempotency, and transition metadata.
- Add static preflight manifests for every contribution. Require trusted-code
  admission or verified confinement before probe execution, then start dynamic
  protocols in quarantine without brokered host-effect leases to negotiate and
  freeze declarations before graph admission.
- Build and validate the candidate graph before ordinary activation.
- Reject cycles, ambiguity, missing owners, and unsupported protocol ranges.
- Add readiness deadlines, health degradation, crash/backoff, generation drain,
  quarantine, and forced-cleanup outcomes.
- Make effective graph inspection available in diagnostics and every operator
  surface.
- Make the graph authoritative for composition and activation; legacy dispatch
  remains explicit until Slice 3 binds calls to graph owners and generations.

Exit gate: registration order cannot select composition/activation ownership and
unresolved required composition cannot publish a runtime. Privileged dispatch
authority has not yet migrated.

### Slice 3: kernel invocation pipeline

- Move permission/RBAC/approval combination, execution leases, scheduling,
  progress, terminalization, and retry classification out of `loop.rs` and
  `EventBus` into one service.
- Replace tool-name policy with declared effects and capabilities.
- Bind every call to owner and graph generation.
- Persist `Prepared` before leasing authority and `Dispatched` before transport
  handoff; carry stable call/deduplication IDs through owner protocols.
- Classify post-dispatch disconnect without authoritative acknowledgement as
  unknown completion.
- Require owner-enforced idempotency or deduplication before retrying mutating
  calls.

Exit gate: no model tool, operator action, trust-boundary call, privileged
internal call, extension effect, or generic slash tunnel can bypass the same
admission and lease path. Pure computation and read-only typed service queries
remain direct.

### Slice 4: provider and loop seams

- Define provider contributions that bind identity, model inventory, auth class,
  schema dialect, bridge factory, and fallback compatibility.
- Make one route service authoritative across interactive, daemon, child, and
  bounded execution.
- Reduce `loop.rs` to a coherent driver over session, route, context, and
  invocation services.
- Make the loop submit transition intents to the kernel session state machine;
  it does not independently mutate canonical state or publish completion.
- Keep the default driver release-coupled; replacement occurs at boot or a
  quiescent session boundary, not mid-turn.

Exit gate: the loop has no concrete provider, tool, memory, lifecycle, or
frontend names.

### Slice 5: complete semantic event spine

- Extend the minimum supervisor event vocabulary into the complete append-only
  session event contract and compatibility rules.
- Treat current persistence as plural input to migration: whole-file LLM-view
  snapshots, metadata checkpoints, narrative journal, and audit streams have
  different schemas and none is semantic replay authority by itself.
- Record model-visible context provenance, route generation, tool schema
  generation, calls/results, and terminal boundaries.
- Derive provider history, transcript surfaces, snapshots, and compaction
  checkpoints from the semantic record.
- Add crash/interruption closure and replay tests before changing storage.

Exit gate: late or restarted consumers reconstruct honest state from snapshots
and cursors without depending on missed broadcasts.

### Slice 6: extract optional domains

- Convert memory, lifecycle, plans/work, behavior, context/compaction, codescan,
  and Git integration to declared in-process services.
- Remove concrete feature imports from semantic surfaces.
- Unify native extension, MCP, and manifest discovery under the contribution
  graph while retaining transport-specific adapters.
- Move shipped skills, prompts, personas, tones, workflows, and catalog data into
  independently versioned content packs.

Exit gate: optional domains can be absent or fail independently without blocking
the maintenance executable or normal kernel startup.

### Slice 7: full artifact and release separation

- Extend the Slice 0 maintenance artifact into the final artifact matrix only
  after contracts and ownership are proven.
- Produce signed contribution locks with identity, digest, protocol range,
  target support, required/optional state, and fallback behavior.
- Test source, linked development, archive, Homebrew, and CI compositions for
  equivalent required modules.
- Enforce dependency, binary-size, startup-task, schema-token, and default
  capability budgets.

Exit gate: release packaging cannot silently produce a partial product and the
minimal kernel artifact contains only constitutionally required behavior.

## Design laws

1. **Contract before extraction.** Move authority to a typed seam before moving
   code or artifacts.
2. **Kernel grants authority.** Contributions describe capability and request
   host effects; they do not self-authorize.
3. **One owner per resource.** Every process, task, registration, subscription,
   durable write, and call has one owner and generation.
4. **One supervisor per session.** Frontends and hosts do not invent independent
   active-turn truth.
5. **DAG before activation.** Static preflight is pure; dynamic discovery occurs
   only in quarantine without host-effect leases. Ordering is deterministic,
   dependencies are acyclic, and promotion is atomic.
6. **Generation-bound execution.** A call cannot cross implementation
   generations; promotion is atomic and old generations drain or revoke under
   explicit policy.
7. **Unknown means denied.** Unknown capability, effect, schema, owner, or
   provenance receives no privileged lease.
8. **Durable facts before broadcasts.** Authoritative state is recoverable from
   versioned records and snapshots; live events are projections or
   interception points.
9. **No retry without semantics.** Mutating calls are not retried after unknown
   completion unless an idempotency contract proves safety.
10. **Semantic surfaces only.** Contributions emit typed DTOs; renderers do not
    infer producer, authority, or actions from strings.
11. **Independent maintenance is permanent.** No optional contribution or
    normal integration startup path can become required to diagnose, deny, or
    quarantine that contribution.
12. **Absence is tested.** Every optional system has a matrix proving kernel
    startup and bounded degradation without it.
13. **Documentation co-ships.** Every lane updates and validates its durable
    architecture/developer docs and applicable public site pages, canonical
    snippets, migration guidance, and operator warnings before its exit gate.

## Acceptance gates

The decomposition may proceed only while these remain true:

- registry tests reject duplicate IDs, ambiguous invocation names, cycles,
  missing dependencies, unsupported protocol ranges, and undeclared effects;
- randomized admission tests prove downstream policy cannot widen an upstream
  denial;
- every advertised model tool and operator action dispatches through the same
  generation snapshot and lease path;
- active calls remain bound to their original generation during replacement;
- candidate activation failure leaves the previous generation callable and
  cleans all candidate resources;
- cancellation, timeout, failed startup, and shutdown tests prove process-tree
  cleanup for transports inside the host's ownership boundary; cross-boundary
  transports report degraded/unverified cleanup and are denied in strict
  profiles;
- confinement tests attempt direct filesystem, process, secret, and network
  bypasses; a contribution requesting confinement fails admission when the
  required OCI/OS boundary is unavailable;
- mutating protocol calls have explicit at-most-once or idempotent retry
  behavior;
- TUI, ACP, Web, IPC, CLI, and headless adapters pass semantic snapshot and
  action parity fixtures;
- late, lagged, disconnected, and restarted consumers recover from snapshot
  plus cursor;
- the separate maintenance executable boots with zero ordinary plugins and can
  inspect its compiled composition, inert contributions, session framing,
  durable ownership records, release artifacts, and audit state; disable or
  quarantine contributions; quarantine sessions; prune proven-stale ownership records;
  and verify explicit release archives without loading corrupt normal-runtime
  state;
- each extracted subsystem retains an in-tree reference implementation through
  at least one compatibility window;
- minimal and full composition matrices remain release-gated.
- every lane reconciles implemented behavior with source design, OpenSpec,
  durable docs, and applicable public site/snippet output before completion.

## Non-goals

- A Rust dynamic-library ABI.
- Reimplementing Cordis or adopting a universal service locator.
- Hot-reloading the loop, supervisor, admission combiner, or persistence
  protocol during active turns.
- Treating manifest declarations as sandbox enforcement.
- Making every crate a separately distributed package.
- Forcing tools, commands, skills, services, projections, and transports into
  one invocation interface.
- Moving code merely to reduce line counts in `main.rs`, `loop.rs`, or
  `tui/mod.rs`.
- Allowing model inference, plugins, or content packs to grant trust.

## Immediate next implementation slice

Slice 0 is decided: produce and package the separately runnable maintenance
executable before changing runtime authority. It should not extract memory,
lifecycle, normal tools, or the TUI; it should prove that recovery no longer
depends on successful startup of the system being repaired.

The next design gate is the remaining frontmatter question: specify the minimum
semantic session event vocabulary needed by Slice 1. Supervisor implementation
must not become authoritative until those prompt, queue, cancellation,
invocation, terminal, cursor, and snapshot-reconstruction contracts are agreed.

After that gate, the sequence above remains authoritative. Optional domains move
only after the maintenance artifact, durable session authority, contribution
graph, and invocation pipeline exist.
