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

At that baseline, admission was layered but incomplete. Secret guards, Styrene
role checks, configured permission rules, host approval, and path-boundary retry
were combined inside tool dispatch. Unconfigured and unknown tool names
defaulted to allow in permission policy and RBAC mapping, and subject extraction
recognized selected tool names. Slice 3 now resolves privileged calls against
the accepted capability graph and denies unknown owners, capabilities, or
effects before permission-policy defaults can grant execution.

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
projection. Slice 3 now makes the accepted graph authoritative for privileged
invocation leases and dispatch; the capability inventory remains evidence and
is not itself an execution grant.

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
transport handoff. A stable call ID and optional owner-enforced deduplication ID
cross owner boundaries. Every unsettled `Dispatched` or `Acknowledged` call is
conservatively unknown completion after recovery. A mutating unknown cannot be
retried unless its original contract proves idempotency or exact stable-call
deduplication. If acknowledgement, unknown classification, or terminal
settlement cannot be persisted after dispatch, the kernel fences further
mutation, writes an emergency recovery record through its last-resort channel,
and does not report ordinary completion.

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

The pipeline now admits and validates invocation kinds beyond model tools.
Graph-registered feature commands from TUI, CLI remote execution, and ACP use
explicit operator principals and declared surfaces, while model-loop path grants
use an internal principal and inherit the parent session/turn authority. Both
paths acknowledge and settle before returning and close their leases exactly
once. Non-TUI control forwarding retains its surface; TUI and CLI feature
bridges are leased, and owner surface declarations now extend that path to the
existing Web and IPC bridges. Automatic memory ingestion and host-mediated
persona/tone switches use internal bindings and leases, and memory mutations no
longer claim read-only orientation. Managed-delegation tools admit declared
service principals on Web/Daemon surfaces and no longer dispatch directly.
Operator context-pack reads use a typed read-only context service rather than
entering tool admission.
The extension-provided voice stop declares TUI service authority and executes
under the promoted turn's durable scope.
Daemon vox polling invokes the declared `vox_route` tool under an ephemeral
Service/Daemon lease and projects the result into the existing event envelope.
Arbitrary ACP methods use one extension-owned conservative Operator/ACP
transport capability and dispatch on the worker-owned EventBus; the current
protocol does not pretend to know per-method effects.
Lease-less imperative extension HostActions fail closed, and operator approval
does not manufacture project, runtime, or trusted-origin authority.
Declarative native HostActions and MCP review candidates require a host-only
parent guard that checks live dispatch state, conservative effect containment,
and exactly-once child identity.
Idle and post-loop calls remain ephemeral rather than receiving fabricated turn
authority. Slice 3's privileged compatibility-path migration and co-delivered
permission, retry, unknown-completion, and recovery documentation are complete.

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

Slice 6.1 begins with a declared-service substrate rather than another file
move. A release-coupled in-process service publishes a bindingless
`in_process_service` capability and exactly one typed implementation under the
same contribution generation. Candidate graph validation, implementation
parity, dependency activation, readiness, and typed-registry publication are
atomic. Handles retain capability, owner, and generation identity and are
captured only at boot or a declared quiescent boundary. Optional absence is
typed local degradation, not synthetic active health. Retirement must settle
generation-owned resources; services with none use strict no-resource teardown.
Slice 6.1.1 implements this atomic typed registry only for no-resource read
services and adds work-source error isolation; no production optional domain is
registered yet. Resource-bearing services remain blocked until lifecycle-owned
drain and cleanup evidence replaces declaration-only promises.

The second no-resource lane is stateless behavior policy. The optional
`service:behavior-policy` / `interface:omegon-behavior-policy-v1` service accepts
immutable host-normalized per-turn views and computes advisory classifications,
evidence, pressure, and recovery decisions without retaining state. Explicit
operator mode parsing, declared tool capabilities, authoritative observation
normalization, operator-correction recovery, durable intent, controller and
recovery counters, tool execution, events, and nudge insertion remain host-owned.
Normal composition preserves canonical parity fixtures BP01-BP09 from the
source design, pinned to the direct implementation at commit `9c3a9860`, and
task 6.1.5 materializes literal service vectors while compatibility tests retain
the direct baseline.
During absence, hosts preserve intent,
hold policy counters, and omit only behavior-policy-derived pressure and meta
retries; operator-correction recovery, completion reconciliation, plan
reminders, stuck recovery, and text-only recovery remain active. All loop hosts
carry the same optional binding through `LoopCompatibilityBindings`, including
ACP worker transfer. A present binding retains service identity; absence is
`None` plus graph degradation and never fabricates an owner or generation. Each
session keeps its own policy state. Like the work snapshot service, this
service owns no task, subscription, process, temporary artifact, or durable
writer. Resource-bearing domains remain deferred until generation-bound drain
and cleanup exist.

Slice 6.1.6 supplies that prerequisite without moving a production domain. It
keeps the existing no-resource services unchanged and adds a separate managed
service class whose implementation never escapes as an unrestricted `Arc`.
An object-safe request/response/error contract runs only through
`ManagedServiceHandle::invoke(request)` in a generation-owned task with
cancellation, panic handling, and active-call accounting; no consumer receives
the implementation by reference or `Arc`. Stale handles retain identity but
return typed draining, degraded, or retired errors without switching owners.
Candidate resources remain unpublished and roll back independently of the
active graph. Synthetic managed generations stage under an owning feature name.
The asynchronous EventBus finalizer derives their composition, owner, and
contribution-generation identities from the prepared graph. It validates graph,
implementation, resource, transition-policy, and capability-ownership parity
before publication. The existing synchronous finalizer remains the no-resource
path and fails closed while graph-managed generations are staged or active.

All handles consult one shared admission table keyed by contribution generation.
After fallible preparation, replacing that complete table is the publication
linearization point. Calls racing it use the old or new table, never a partially
closed set; pre-point failure rolls back the candidate, while post-point cleanup
degradation cannot falsely claim rollback. EventBus commits the already-prepared
graph and compatibility caches without suspension after that swap. The
active-call deadline starts at
the table swap. Remaining calls are cancelled, aborted, and joined before a
separate cleanup deadline begins. Resource controllers use a validated
dependency DAG; stop, conditional force-stop, and settlement run in reverse
topological order without resetting the deadline per resource.

Strict cleanup requires positive settlement before retirement or ownership
release. Timeout or failure is nonterminal degraded cleanup with retained owner
and bounded evidence of attempted stop/force-stop; cross-boundary best-effort
cleanup may be unverified. A later retry can finish retirement. Unchanged
contribution generations transfer without cleanup. Their canonical lifecycle
and resource records retain the composition generation that originally admitted
the physical owner; the containing composition projection identifies the current
accepted composition. Boot-only service changes are always rejected after first
publication. Quiescent-declared replacement requires a current
runtime/session-bound one-use proof; this substrate ships no production proof
issuer or migration command.

EventBus retains cleanup tasks and lifecycle records, serializes replacement
with explicit async shutdown, and prevents caller cancellation from detaching
work. Diagnostics retain a bounded DTO-only history of published and rejected
attempts and project actual lifecycle, resource settlement, stop, force-stop,
and bounded reason evidence through the shared composition surface. Interactive,
daemon, headless, bounded, Sentry, ACP worker, cleave, and injected-runtime hosts
await managed shutdown before releasing runtime ownership. Clean shutdown
removes process ownership only after every resource settles; degraded or
unverified owners remain retained for retry. Drop can request
cancellation/force-stop but cannot claim settlement. The RG01-RG12 synthetic
campaign proves the shared machinery. Codescan is the first production managed
lane because its index is rebuildable and workspace-scoped, with no required
subprocess or session authority. One serial worker now owns SQLite, indexing,
HEAD freshness, and BM25 construction for tools and code-context requests.
Its strict task resource depends on its strict durable writer. Shutdown joins
the worker before SQLite settlement. Memory, lifecycle, Git, and
context/compaction remain deferred.

Slice 6.1.8 freezes lifecycle/OpenSpec as the next managed lane. One boot-only
`service:lifecycle` / `interface:omegon-lifecycle-v1` repository worker owns the
loaded opsx FSM/ledger, design and OpenSpec artifact coordination, repository
revision, reconciliation, recovery journals, and every Omegon-authored lifecycle
mutation. Git-native artifacts remain semantic content authority; the ledger is
enforcement and audit state. Reads and mutations return owned revisioned DTOs,
and consumers cannot retain the implementation, provider locks, artifact
repositories, or direct filesystem fallbacks. Session focus, context TTL,
rendering, authorization, TDD evidence, Codex export, arbitrary prose authoring,
and stopped-runtime migration remain with their existing session, host, or
adjacent durable owners.

The lifecycle worker is a strict task depending on a strict durable writer.
Revision checks protect external Markdown edits from stale overwrite, while
stable operation identities make ambiguous request replay idempotent. Failed
in-memory FSM saves must restore their pre-operation state. Multi-resource
design/OpenSpec/ledger changes use versioned repository-relative journals and
deterministic recovery to reach one complete pre-operation or post-operation
state. Optional absence keeps tools and semantic surfaces declared with typed
unavailability, omits only lifecycle-derived data, and never reconstructs an
independent ledger or scanner in ACP, work aggregation, workflow, or another
consumer. The managed read/recovery owner now publishes this worker with
deterministic ledger, artifact, and transaction revisions, boot-captured typed
handles, and strict worker-before-writer settlement. Conflicting populated
OpenSpec roots disable lifecycle before compatibility registration. Design
mutations now use frozen-root, revision-checked artifact-plus-ledger journals
with exact pre/post identities, deterministic roll-forward recovery, durable
operation receipts, and typed stale/conflict/recovery errors. OpenSpec proposal,
spec, task, test-registration, transition, archive, abandon, and reopen mutations
now use the same revision and operation-identity boundary. Archive and reopen
journals contain bounded repository-relative resources, exact tree identities,
ledger-last settlement, and deterministic recovery. Recovery validates receipts
before writes and quarantines malformed, unhealthy, oversized, path-tampered, or
unknown frontiers. A populated legacy OpenSpec root remains selected when the
primary root contains only metadata.

Production consumers now capture the accepted lifecycle binding at boot or use
an immutable host-owned repository observation. This boundary covers lifecycle
tools and doctor, context, setup snapshots, ACP, TUI, Web, IPC, workflow,
Sentry hooks, work aggregation, project rules, session/check-in projections,
prompt guidance, startup status, and Codex export. Work aggregation publishes
its immutable snapshot from the managed observation within the original boot
composition. Sentry design transitions and one-shot CLI checks create and shut
down a managed lifecycle composition instead of scanning or writing artifacts
directly. Source guards keep direct repository constructors and canonical-path
scanners out of production consumers. Direct compatibility adapters remain
available only to tests. External Markdown authoring, append-only TDD evidence,
and stopped-runtime migration remain explicit adjacent boundaries.

Slice 6.1.9 freezes memory as the next managed lane. One boot-only
`service:memory` / `interface:omegon-memory-v1` worker owns the selected project
store, optional global store, SQLite connections and WAL state, durable facts,
minds, edges, episodes, vectors, JSONL import/export, and configured Codex-vault
synchronization. Consumers capture one generation-tagged handle at boot or use
owned output from that handle. They cannot retain `MemoryBackend`, a database
connection, a vault writer, or a callback that exposes the implementation.

Memory mutations use stable operation identities. Content-addressed stores keep
their natural idempotency, while targeted mutations use entity-specific version
preconditions. Independent fact writes do not contend on an artificial global
revision. JSONL import retains its per-record Lamport conflict policy. SQLite
changes that span facts, edges, vectors, or metadata remain atomic. Embedding
generation stays outside the service, and vector failure preserves deterministic
FTS retrieval. The service stores provider results but does not select providers,
retain credentials, or execute extraction and embedding policy.

The host retains session-local working-memory pins, selected mind, context TTL,
context hashes and presentation policy, tool rendering, authorization, frontend
state, provider tasks, and compaction. Durable minds and parent relationships
remain service-owned. Host-owned provider tasks must be bounded and joined before
managed shutdown, and they persist only through the captured handle. The strict
memory worker depends on one strict durable writer. Shutdown stops and joins the
worker before SQLite/WAL, JSONL, and vault writes settle. Stopped-runtime schema
or selected-root migration remains outside the service. Embedding backfill uses
a bounded managed composition rather than opening SQLite directly.

Compatibility includes the existing selected-root policy, persisted schema and
wire vocabulary, SQLite/in-memory parity, retrieval and decay behavior, minds,
graphs, episodes, JSONL merge behavior, vault path safety, tool contracts,
context ordering, and status projections. If memory is absent, tools and status
remain declared with typed unavailability, durable memory context is omitted,
and unrelated context and host-owned compaction continue without a direct store,
JSONL, or vault fallback.

Task 6.1.9.1 establishes the durable prerequisite. Schema v8 adds compact
payload-bound operation receipts and complete episode metadata. A stable
operation identity replays its recorded effect only when the payload hash
matches; conflicting reuse fails without mutation. Targeted operations use fact
version preconditions, while independent writes and JSONL imports do not share a
global revision. SQLite transactions now cover mind-plus-record creation, batch
status changes, supersession, vector-plus-model metadata, episodes, and JSONL
imports. The in-memory backend stages the same compound effects before
publication. Both backends preserve Lamport high-water marks, deterministic FTS
tie ordering, JSONL rollback/idempotency, and episode metadata. Migration accepts
schemas v5-v7: v5/v6 `default` records remain quarantined in `legacy`, and v7
`default` records reconcile to `primensus`.

## Selective decomposition map

| Current subsystem | Target tier | First boundary to establish |
|---|---|---|
| `RuntimeCapabilityRegistry` | Kernel | Declarations become pre-activation input and dispatch authority rather than a read model |
| `EventBus` | Kernel registry adapter, then service/event adapter | Remove hard-coded tool classes, timeout names, disabled-name authority, and collision-by-order |
| Interactive coordinator/runtime supervisor | Kernel | Compile one frontend-neutral implementation and instantiate it once per session across hosts |
| `loop.rs` | System loop driver plus kernel invocation client | Extract admission, tool scheduling, host effects, compaction, and feature-specific requests |
| Conversation/session | Kernel event contract plus replaceable projections/storage | Inventory whole-file LLM-view snapshots, metadata checkpoints, narrative journal, and audit log; define semantic events before migration |
| Provider routing | System service | One provider declaration owns identity, auth class, inventory/evidence authority, tool dialect/support, bridge factory, and fallback compatibility |
| Core tools | System/in-process services | Replace name switches with declared effect and execution metadata |
| Permissions/RBAC/secrets | Policy providers plus kernel combiner/effect executors | Deny unknown effects and bind decisions to owner/generation |
| Memory | In-process service | Remove concrete `memory_store` knowledge from loop and provider resolution from feature |
| Lifecycle/OpenSpec/design | In-process service | Expose read/mutate/projection contracts without kernel or surface imports of concrete feature types |
| Plans/Workbench/work runtime | In-process aggregation plus semantic projection | Separate session-local plan authority, lifecycle artifacts, and Workbench read model |
| Codescan | Managed in-process service | Keep one boot-captured handle and one serial owner for SQLite, scanning, freshness, and BM25 |
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

- Define provider contributions that bind identity, model inventory/evidence
  authority, auth class, schema dialect/tool support, bridge factory, and
  fallback compatibility.
- Make one route service authoritative across interactive, daemon, child, and
  bounded execution.
- Reduce `loop.rs` to a coherent driver over session, route, context, and
  invocation services.
- Make the loop submit transition intents to the kernel session state machine;
  it does not independently mutate canonical state or publish completion.
- Keep the default driver release-coupled; replacement occurs at boot or a
  quiescent session boundary, not mid-turn.

Task 4.1 now provides one validated built-in provider registry in the integration
crate. Each declaration has a typed contribution owner/generation, runtime
inventory binding, credential class, executable tool contract, bridge-factory
binding, offering modality/capability evidence requirement, and directed
model-family fallback relations. Candidate validation reports all missing
semantics and rejects duplicate IDs/aliases, inventory-owner mismatches, and
dangling/self fallback targets. The registry now owns known-provider parsing,
schema lookup, family fallback, and bridge-factory selection while retaining the
existing provider clients and host route adapters. Google API-key execution is
truthfully declared as the current OpenAI-compatible adapter, Moonshot retains
its full-schema exception, Ollama Cloud declares tools unsupported, and the
currently unavailable Antigravity factory remains non-executable rather than
masquerading as a credential failure.

Task 4.2 now routes bridge and declared-fallback resolution through one
`ProviderRouteService`. Compatibility bridge handles retain both selected and
serving identities, so interactive, daemon, ACP, child, bounded, smoke,
compaction, and lightweight completion dispatch no longer reconstruct fallback
identity in host adapters.

Before each provider stream, the runtime records a versioned route lease with
selected and serving provider/model identities, schema dialect or unsupported
state, credential-source class, bounded fallback reason, provider-contribution
generation, and route policy. Session-backed requests append
`route.lease_recorded` to the active turn's authority stream. Sessionless work
uses a durable step-owned JSONL stream under the Omegon runtime home and never
fabricates session or turn authority. Missing durability, partial session scope,
stale contribution generation, and undeclared fallback all prevent dispatch.
Provider retries reuse the request's captured lease for the same serving route;
route re-resolution necessarily captures current contribution evidence.

Task 4.3 places the compiled loop behind one release-coupled driver. Every
interactive, ACP, daemon, headless/child, bounded, and Sentry turn constructs the
same four required trait ports for session state, leased provider route, context
assembly, and privileged invocation. The driver captures route and invocation
authority before entering loop policy; partial session authority, a stale active
turn, cross-session authority, missing serving identity, or disagreement between
the route controller and bridge fails before execution.

The driver returns a typed terminal proposal rather than requiring each host to
reclassify ordinary success, provider exhaustion, and failure. Authority-backed
hosts may still narrow that proposal for an admitted cancellation or bounded
timeout, and commit it only after host-owned cleanup. `TurnEnd` and `AgentEnd`
remain advisory projections, not canonical completion.

Task 4.4 now keeps the release-coupled compatibility implementations outside
production loop policy. The driver captures one opaque host binding and adapts
the existing session projection, context manager, invocation runtime, and leased
provider bridge behind the four required contracts. Provider route policy is
snapshotted by the route port rather than calling back into `LoopConfig`; tool
admission/batching/owner handoff, permission presentation, memory/lifecycle
requests, context assembly/compaction, and plan/recovery/finalization policy stay
in their owning adapters. Source guards reject concrete names, direct authority
bypasses, and route or invocation adapter callbacks into loop orchestration.
Task 4.5 now gives each durable session one execution owner for an atomic driver
and route-service pair. Turn start captures the pair under the owner's migration
gate. Mid-turn replacement returns in-memory `Pending` and cannot alter the
active capture; neither ordinary closure nor the next start applies it. Only a
deliberate `commit_pending_at_quiescence` call can append the migration and then
publish the pair. Idle state, exact process-local and durable source, and the
absence of every unresolved or unknown invocation are required. Durable failure
retains the old pair, concurrent start and migration cannot mix pair members,
legacy replay invents no binding, and resume boot binding appends no migration.
All durable hosts consume the session capture; sessionless headless/Sentry,
smoke, compaction, and lightweight routes consume the immutable boot binding.

Task 4.6 closes Slice 4 documentation. The canonical provider-contribution and
route-lease guide, related credential/fallback/session/recovery architecture,
public site, README, and release notes now distinguish declaration, inventory,
authentication, executable routing, selected and serving identity, interactive
and sessionless fallback scopes, provider retry, and invocation replay. Public
claims explicitly retain non-executable Antigravity, warning-only headless
Anthropic subscription behavior, advisory inventory diagnostics, directed
non-transitive fallback, bounded credential/authentication-class evidence, and
the absence of historical lease commands or an exhaustive current inventory.
No command or configuration syntax changed, and canonical site snippets remain
unchanged. Full message/context/tool/step persistence remains Slice 5.

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

Task 5.0 freezes the first production lane. One internal `loop.rs` iteration is
one durable step. Context-overflow or provider-history repair closes the current
request and creates the next request plus route lease inside that same step. The
existing `route.lease_recorded` v1 remains unchanged; the new
`model.request_route_joined` fact supplies request/step linkage.

The exact task-5.1 v1 additions are `step.started`,
`model.request_prepared`, `model.request_route_joined`,
`assistant.content_appended`, `assistant.message_committed`,
`provider.continuity_stored`, `tool.call_recorded`, `tool.result_recorded`,
`model.request_closed`, `step.closed`, and `step.abandoned`. They use UUID
entities, contiguous turn/step/request/content/call/result ordinals, immutable
context manifests, and a schema-set digest over canonical composition plus
composition/owner generations. Denied calls and denied results are canonical;
admitted calls/results must agree with existing invocation facts.

Assistant display output is appended and synced as bounded, coalesced chunks
before broadcast, not synchronized once per provider token. Model-visible bytes,
schema bytes, assistant chunks, call arguments, and results use a
session-adjacent content-addressed blob store with verified digest, media type,
length, storage class, and projection class. Hidden reasoning or opaque provider
continuity is stored only when required for continuation, in restricted blobs
excluded from default snapshots, transcripts, UI/ACP, diagnostics, exports,
memory, tools, and extensions. Arbitrary raw provider payload storage is
prohibited. The captured provider contribution must declare generation-bound
continuity policy `none` or `restricted_required`, including allowed kinds and a
blob ceiling; this policy remains outside the unchanged route-lease v1 payload.

Task 5.1 now emits this vocabulary in production only for complete
authority-backed sessions. Interactive, ACP, daemon, and bounded hosts perform
deterministic abnormal terminalization after owned cleanup and before turn
closure; recovery uses the same invocation-first, request-first ordering.
Partial scopes fail closed, sessionless hosts retain route-only evidence, and no
tool progress facts are added. Full compatibility
and replay matrices are complete in 5.2; provider-history/transcript/frontend/compaction
shadow derivation is complete in 5.3; legacy storage/consumer migration is complete
in 5.4; sessionless semantic lineage is deferred; lagged, disconnected, corrupt, and restarted
consumer fixtures remain 5.5. This activation changes no
command, configuration, public site page, or canonical snippet.

Task 5.2.0 froze the next lane without changing runtime. Authority lineages are
forward-only after their first full-spine fact: older readers fail closed, old
writers cannot run concurrently or downgrade the lineage, and legacy transcript
bytes are never synthesized into authority. The compatibility matrix has three
states: legacy has no semantic provider-history/exact-export claim; mixed has an
explicit boundary and exact full-spine suffix only; full has semantic authority
from its first eligible operation. Missing or tampered referenced blobs fail
full recovery closed. Diagnostics alone may show unavailable placeholders.

The next bounded task-5.2 component now provides a kernel-internal read-only
replay API rather than exposing mutable session authority or JSONL parsing.
Stable selected prefixes are strictly decoded and reduced to immutable typed
records and an exact session/stream/sequence/event frontier; every referenced
blob verifies before success. Foundational reducer/cache v5 state classifies
legacy-only, mixed, and full-spine lineages and records the first full-spine
boundary. Default content reads cannot dereference restricted continuity, whose
read authorization binds the target request and serving lineage. Replay reports
open, prepared-only, unknown, and abandoned state as durable evidence and never
appends recovery facts or consults transcript/cache authority. The canonical
fixture directory freezes concise JSONL and blob/meta bytes, with deterministic
builders for longer semantic spines and corruption recipes.

The frozen additions are `context.source_materialized`,
`model.response_attempt_failed`, and the `compaction.started`,
`compaction.request_prepared`, `compaction.response_attempt_failed`,
`compaction.request_closed`, `compaction.summary_committed`,
`compaction.applied`, and `compaction.abandoned` v1 family. Manual idle
compaction is session-scoped and
does not invent a prompt, turn, step, or turn-owned route lease. It holds the
supervisor admission gate and atomically replaces derived context only after the
applied fact is durable. Generic projector cursor v1 binds projector/schema
versions, source cursor, output revision, and output digest; synced output must
be atomically published before its cursor. The bounded generic cursor substrate
now validates exact, stale, and invalid replay frontiers and publishes
deterministic bytes through restrictive, crash-safe output-before-cursor storage.
It does not activate or migrate a concrete projector. Task 5.2 now emits and
reduces `model.response_attempt_failed`, materialized-source provenance, and the
complete compaction authority family; recovery is invariant across durable
compaction/invocation/request/step prefixes. Reducer/cache v5 indexes, strict
replay and blob authorization, the canonical fixture corpus, atomic host session
replacement, and the generic cursor substrate complete the slice. Concrete
provider-history, transcript, frontend, and compaction-checkpoint derivation
belongs to 5.3 and must consume rather than re-emit semantic authority;
legacy consumer migration belongs to 5.4; adverse consumer fixtures belong to
5.5. Sessionless work remains route-only until a separately designed lineage.

Task 5.3.0 freezes that derivation before implementation. The internal projector
IDs are `session.provider-history`, `session.transcript`,
`session.frontend-snapshot`, and `session.compaction-checkpoint`; every projector
and projection schema starts at version 1. A common availability envelope makes
lineage and exactness explicit: full lineages may expose `exact_full`, mixed
lineages expose only `exact_suffix` from the first full-spine boundary and mark
full-session export unavailable, and legacy lineages expose no exact content.
Provider history records immutable exact joined-request inputs, never a
synthesized next-request context. The normal transcript records committed
messages only. Frontend evidence may additionally include durable partial or
abandoned chunks plus queue, active-turn, context, and semantic-conversation
state. Committed content remains visible with abnormal status if its owner is
later abandoned; live tool progress stays a downstream ephemeral overlay.

Provider-history and transcript output is split into immutable bounded chunks
and a bounded manifest under the existing 16 MiB generic output limit. Frontend
and compaction checkpoint are bounded single outputs. All four exclude
restricted continuity bytes and use semantic ordering, RFC 8785 canonical JSON,
and SHA-256 over exact bytes. A session coordinator coalesces wakeups but always
replays the latest stable frontier; each projector publishes independently and
retains its previous stable cursor on failure. At the 5.3 boundary the
implementation was shadow-only: it did
not switch `ConversationState`, provider dispatch, transcript commands, TUI,
ACP, Web, IPC, whole-file snapshots, or compaction compatibility consumers.
Those migrations are now complete in 5.4. The 5.3 freeze changed no runtime or public behavior
and therefore adds no public docs, site, command/configuration/snippet, or
changelog note.

Task 5.3 is now active in shadow mode. Each authority-backed supervisor owns one
capacity-one, dirty-bit worker for its full session lifetime. Hints occur only
after durable append, ordinary bursts use the frozen 50 ms/250 ms cadence, and
terminal, startup/recovery, explicit flush, and shutdown boundaries are
immediate. Every run strict-replays the latest stable frontier and independently
invokes all four projectors. Replacement clears and stops the retired worker,
uses session-specific roots to fence late old-authority activity, and transfers
the join handle to the new supervisor for owned reaping without delaying host
publication; sessionless supervisors start no worker. Typed failures are logged
and inspectable without blocking authority or replacing prior outputs. The
task-5.4 consumers now use the validated reader and remain source-guarded against
direct projection-storage access.

Task 5.4.0 freezes the migration lane. Semantic authority owns replay facts and
the exact committed transcript, but it does not absorb every durable concern.
`IntentDocument` and plans move to a separately versioned host-state checkpoint;
operator observations move to a separately sequenced durable ledger; friendly
name and description remain operator-owned metadata; semantic counters remain
derived; audit remains an independent ledger with cursor/source-event dedup; and
the narrative journal remains Markdown with canonical machine-readable source
provenance on new entries. Sessionless semantic lineage remains deferred.

Provider dispatch does not consume the provider-history projector. Under the
session coordination boundary it materializes attributable host/generated
input, captures synced authority EOF, and synchronously strict-reduces one
bounded immutable `CurrentContextViewV1`. Semantic manifest order is final;
every item carries source-event and owner-generation provenance. Missing blobs,
unsupported facts, unattributed post-boundary input, bound violations, or any
frontier mismatch fail before dispatch. Exact resume uses the same exact-
frontier rule. A TUI, ACP, IPC, Web, catalog, or evidence surface may display a
validated stale projection only with cursor and event-count lag disclosure.

The replacement publication set adds host-state checkpoint/cursor, observation
ledger, operator catalog metadata, derived catalog and telemetry snapshots,
audit v2 source fields, and journal provenance to the authority/blob store and
four existing semantic projections. Validated readers check identity, schema,
canonical digest, cursor/output revision, replay frontier, chunks, and bounds.
Provider dispatch has no fallback; exact transcript/resume synchronously catches
up or fails; presentation surfaces may fail open only to labeled stale or
unavailable state; audit/journal failure cannot block authority.

`/transcript` is reserved for exact committed semantic transcript output.
`/session-export` becomes the distinct current presentation/evidence export and
must disclose partial, abandoned, overlay, lineage, exactness, and freshness
state. Full lineages resume/export exactly. Mixed lineages may resume as an
explicitly labeled immutable legacy base plus exact semantic suffix, but exact
full-session export remains unavailable and Web historical output contains the
suffix only. Legacy resume remains labeled compatibility behavior.

Slice 5.6 closes compatibility publication at semantic self-sufficiency. Full
lineage and mixed lineage with one durable content-addressed legacy base stop
rewriting `.json`/`.meta.json`; legacy and not-yet-materialized mixed sessions
retain the pair only as a one-way importer. Existing artifacts are not deleted.
No rollback selector makes them authoritative, and crossing the full-spine
boundary permanently denies a reduced old writer. New independent schemas remain
at their frozen versions without changing authority/event v1, reducer/cache v5,
cursor v1, or task-5.3 projection v1. Task 5.4 owns consumer cutover, task 5.5
owns adverse-consumer campaigns, and task 5.6 completes compatibility publication
plus public `/transcript`, `/session-export`, session, migration, and recovery
documentation and canonical snippets.

Task 5.5.0 freezes the campaign before implementation. The normative private
protocol defines a closed fault/disposition vocabulary and 54 stable pairwise
scenarios across lineage, lifecycle, and consumer axes. Exact consumers fail
closed; validated projections/frontends may degrade only with cursor, lag,
lineage, and availability disclosure; evidence and mirror consumers are best-
effort only where their failure cannot masquerade as semantic success. Copied
fixture sandboxes and deterministic I/O, notification, worker, and replacement
injection replace sleeps. Required Linux, macOS, and Windows lanes each have a
15-second budget.

The freeze permits only one deterministic repair: a proven corrupt derived chunk
may be quarantined and regenerated from validated authority under its owning
projector lock. Replacement validates authority and required host stores, while
missing or damaged derived projections may remain explicitly unavailable during
new-generation rebuild. ACP completion follows worker/supervisor state despite
skipped notifications and bounded drain; IPC lag automatically enqueues current
reconciled state. Missing observation storage degrades open only with no durable
evidence of prior existence; malformed/torn storage fails closed. Malformed
semantic audit input halts semantic audit advancement and warns without invented
rows. Journal authority read failure is semantic-source-unavailable, authority
with no catalog record is fatal, and semantic durability followed by mirror
failure returns partial publication while preserving semantic resume.

The named runtime red fixes have focused coverage, and all 54 manifest rows
enter exhaustive consumer-specific campaign oracles. AC13 uses a chunk-bearing
mixed-lineage fixture and exercises immutable chunk reconstruction. The macOS,
Ubuntu, and Windows campaigns pass within budget; GitHub Actions run
`32622078435` at `b788f3b8` supplies the required Ubuntu and Windows evidence.
Task 5.5 is complete without revising accepted authority/schema vectors. Task
5.6 completes the frozen one-way compatibility importer and developer/applicable
public session, migration, recovery, site, and snippet guidance.

Exit gate: late or restarted consumers reconstruct honest state from snapshots
and cursors without depending on missed broadcasts.

### Slice 6: extract optional domains

- Establish generation-bound typed in-process service publication before moving
  optional domain code. The first proof is read-only plans/work aggregation over
  `styrene-work-model` and `styrene-work-runtime`; source errors become local
  warnings, immutable snapshots feed shared readers, and service absence leaves
  session-local plan mutation intact.
- Slice 6.1.2 composes the first production half of that proof: an Omegon-owned
  OpenSpec/design adapter publishes one immutable boot snapshot as the optional
  `service:work-snapshot` in-process service. Shared CLI/loop/ACP reader cutover
  remains separate, so direct compatibility scanners still exist.
- Slice 6.1.3 derives the existing repository plan/task read model from that
  snapshot with compatibility parity, including stable-ID diagnostics. It does
  not yet switch production readers.
- Slice 6.1.4 captures the accepted service at boot and routes CLI, loop, and ACP
  repository-plan reads through the shared snapshot projection. If the optional
  service is absent, session plans remain available and repository arrays are
  empty; production readers do not rescan the filesystem.
- Slice 6.1.5 is frozen as a stateless behavior-policy service. It preserves
  named direct-policy behavior when present; when absent, loop execution holds
  policy counters and omits policy-derived pressure/meta retries without
  suppressing host-owned operator-correction, completion, plan, stuck, or
  text-only recovery.
  Explicit mode parsing, tool capabilities, observation normalization, session
  intent, controller/recovery state, tool execution, events, and nudges remain
  host-owned.
- Slice 6.1.6 adds generation-bound managed calls, call drain, strict resource
  cleanup, retained degradation evidence, and explicit host shutdown.
- Slice 6.1.7 publishes codescan as the first resource-bearing production
  managed service. Tools and code-context requests share one boot-captured
  handle and one serial worker. Optional absence remains typed and local.
- Slice 6.1.8 publishes lifecycle/OpenSpec as one revisioned managed repository
  service. One boot-only serial worker owns revisioned reads, mutations, health,
  and safe recovery behind a typed handle with strict worker/writer cleanup.
  Design and OpenSpec mutations use crash-recoverable artifact-plus-ledger
  journals and idempotent receipts. Production consumers use captured handles or
  immutable observations. Git-native content authority, host/session state, and
  external authoring remain outside.
- Slice 6.1.9 freezes memory as one managed durable service. One serial worker
  owns project/global stores and JSONL/vault persistence. Session context and
  provider computation remain host-owned, deterministic FTS remains available
  without embeddings, and optional absence has no direct storage fallback.
  Its first implementation checkpoint establishes transactional schema-v8
  persistence, payload-bound operation replay, backend parity, governed v5-v7
  migration, deterministic fallback, and reopen/rollback evidence.
- Convert memory, context/compaction, and Git integration to declared
  in-process services.
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
- the frozen 54-case late, lagged, disconnected, restarted, replacement, and
  corruption campaign preserves each consumer's fail-closed/degraded/best-effort
  law and operator-agency invariants;
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
