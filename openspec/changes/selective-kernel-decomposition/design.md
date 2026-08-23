# Design: selective Omegon kernel decomposition

## Source design

- `docs/selective-kernel-decomposition.md`
- `docs/omegon-maintain.md`
- `docs/binary-composition-and-kernel-admission.md`
- `docs/harness-architecture-parity/deepseek-harness.md`
- `openspec/archive/runtime-capability-declarations/`

## Decision

Adopt "everything optional attaches through a contract," not "everything is an ordinary plugin."

The constitutional kernel owns only identity, contribution lifecycle, durable session transition truth, admission combination, invocation leases, event ordering, process/resource ownership, and recovery bootstrap. Default loop behavior, providers, tools, memory, lifecycle, planning, context policy, orchestration, and frontends are system modules or selectively composed services. Third-party code remains separated by explicit trust and transport boundaries.

## Lifecycle status

This change remains `proposed` overall. Slices 0 through 5 are complete; optional domain extraction remains open. Slice 5.5 gives all 54 rows exhaustive scenario-specific executors, includes AC13's chunk-bearing mixed-lineage rebuild fixture, and passes the focused Ubuntu/Windows campaign matrix. The macOS, Ubuntu, and Windows campaigns are evidenced within budget; GitHub Actions run `32622078435` at `b788f3b8` supplies the required Ubuntu and Windows evidence. Slice 5.6 closes compatibility publication at the frozen semantic self-sufficiency boundary, migrates maintenance to catalog-first framing, and publishes the applicable public/developer documentation and canonical snippets. Slice 6.1 has a frozen declared-service contract; Slice 6.1.1 implements atomic no-resource typed-service publication and work-source error isolation, but no production optional domain is registered or cut over yet. Homebrew publication is not a Slice-zero exit gate; existing formula verification remains a best-effort packaging safeguard. Pairwise install, self-update, and version switching publish immutable complete generations and select the executable pair plus receipt through one atomic activation link. Each later slice begins with an explicit refinement gate that names concrete ownership, compatibility boundaries, red tests, and documentation impact before production mutation.

## Architectural layers

### Constitutional kernel

The kernel owns:

- stable runtime, contribution, capability, session, prompt, turn, invocation, lease, and generation identities;
- renderer-neutral protocol and semantic event vocabulary;
- contribution dependency validation, generation construction, promotion, drain, rollback, and retirement;
- one session state machine and supervisor implementation, instantiated once per session;
- monotonic admission combination and privileged execution leases;
- crash-consistent invocation state;
- semantic event sequencing, snapshot cursors, and terminal closure;
- host-owned process/resource supervision and machine-readable recovery diagnostics.

These are release-only authorities. They may be split into internal crates, but they are not ordinary operator-installed plugins.

### Release-coupled system modules

System modules use the same declaration and lifecycle vocabulary while retaining privileged, in-tree status:

- the separately runnable `omegon-maintain` companion executable;
- the default loop policy driver;
- provider route selection and route leases;
- provider-history and transcript projections;
- filesystem, process, network, terminal, secret, and resource host-effect executors;
- required configuration and packaging adapters.

Replacement occurs at boot or a declared quiescent generation boundary, never mid-turn.

### In-process services

Memory, lifecycle, Git, codescan, context/compaction, behavior policy, plans/work aggregation, provider transports, and catalog resolution are statically linked Rust services with explicit contracts, dependencies, and teardown. Extraction from the main crate does not grant third-party replacement authority.

Slice 6.1 distinguishes these services from constitutional `kernel_service` capabilities. A declared in-process service is a bindingless capability paired with exactly one typed implementation in the same candidate generation. Candidate graph validation, implementation parity, dependency activation, readiness, and publication of the typed registry are one atomic boundary. A published handle identifies its capability, owner, and contribution generation; consumers capture it at boot or a separately admitted quiescent boundary rather than looking up a changing ambient implementation mid-turn.

Required and optional service dependencies participate in graph activation and health. Missing or failed optional services publish bounded unavailable/degraded evidence and leave unrelated capabilities callable; they are never represented as active merely because their declaration was accepted. Candidate failure preserves the complete previous graph and service registry. Retirement settles every generation-owned task, subscription, temporary artifact, and durable writer before claiming success. Services with no owned resources declare strict no-resource teardown rather than inheriting a best-effort process cleanup default.

The first proof lane is read-only plans/work aggregation. `styrene-work-model` remains the provider-neutral contract owner and `styrene-work-runtime` remains the refresh/immutable-snapshot owner. Omegon contributes OpenSpec/design source adapters and semantic projection integration. Source errors become source-local warnings instead of aborting the aggregate. The service may be absent without removing session-local plan mutation. This lane does not move `IntentDocument` plan authority, change Workbench or `PlanSurfaceProjection` DTOs, revise durable session schemas, or alter commands and frontend wire behavior.

Slice 6.1.1 publishes only the substrate needed by that lane. The additive service capability carries a stable interface identity and an object-safe implementation holder. EventBus publishes typed handles only after graph and implementation parity pass and rejects any capability, interface, erased type, implementation, or service-set change that reuses an accepted contribution generation. The initial API is intentionally limited to strict zero-timeout no-resource reads. Existing handles may retain their captured immutable generation; tasks, subscriptions, durable writers, and other resource-bearing services cannot use this path until contribution lifecycle retirement supplies real drain and cleanup evidence. No production domain is registered by 6.1.1.

Slice 6.1.2 registers the first production half of that lane. The statically linked `work-aggregation` contribution performs one boot refresh, normalizes OpenSpec and design artifacts, and publishes an immutable `WorkSnapshot` through `service:work-snapshot` / `interface:styrene-work-snapshot-v1`. It owns no task, subscription, watcher, or durable writer. Existing CLI, loop, and ACP repository-plan readers remain on their compatibility scanners until the next bounded shared-reader cutover; therefore service absence is not yet claimed as a complete plans/work extraction.

Slice 6.1.3 adds the semantic conversion boundary without changing consumers. Source facets retain the OpenSpec grouping/order and structured task-identity findings needed to recreate the existing repository plan/task DTOs; duplicate stable markers remain diagnosable while normalized work identities stay unique. The shared `surfaces::plans` adapter has parity coverage against the direct scanner, so the following cutover can replace producer access without changing command or frontend shapes.

Slice 6.1.4 captures the typed work snapshot immediately after accepted composition publication and carries that immutable handle through every loop host and the ACP worker boundary. Shared CLI, loop, and ACP repository-plan readers consume only the captured snapshot. Absence is explicit: session-local `IntentDocument` mutation and rendering continue, repository plans/tasks are empty, and no reader performs an ambient service lookup or direct filesystem fallback. The compatibility scanner remains only as the parity oracle for tests.

### Out-of-process contributions

Native/OCI extensions, MCP, HTTP/OpenAPI, and optional remote providers/tools negotiate versioned capabilities. Unsandboxed native, script, and MCP processes are trusted host-authority code. Least authority is claimed only when verified confinement blocks direct host access and privileged effects cross kernel brokers.

### Content and frontend contributions

Skills, prompts, personas, tones, workflows, and catalog data are content packs. TUI, ACP, Web, IPC, CLI, daemon ingress, and schedulers are frontend/host adapters. Neither content nor frontends own runtime authority.

## Maintenance artifact

Slice 0 adds workspace package `omegon-maintain` at `core/crates/omegon-maintain/`, producing the separate `omegon-maintain` executable rather than a flag or second binary target in package `omegon`. Package `omegon-maintenance-contracts` at `core/crates/omegon-maintenance-contracts/` supplies only versioned deny, session-deny, ownership-record, exclusion-lock, transaction, audit, package-manifest, and canonical-key schemas used by both executables; it does not pull normal runtime code into maintenance. The executable shares the workspace release version, must not depend on package `omegon`, and is packaged beside `omegon` as a required release companion. It excludes normal TUI startup, the default agent loop, provider clients, project configuration evaluation, project contribution or extension code, MCP, mutable packs, memory, lifecycle, and orchestration.

The maintenance artifact supports:

- compiled maintenance-profile identity, exclusions, and bounded inert-entry diagnostics;
- inert contribution list/inspect plus maintenance-owned disable and reversible quarantine;
- session snapshot/metadata framing inspection plus resume-deny quarantine without semantic rewriting;
- durable ownership-record inspection and stale-record pruning without killing arbitrary processes;
- offline signed release archive and companion-artifact verification;
- maintenance audit inspection and verification.

Slice zero explicitly excludes generic read/search/patch/shell, project source or configuration mutation, semantic session repair, contribution enable/purge/install/update, process killing outside the current invocation, network release discovery/download, installation activation, update, and rollback. Later slices establish some prerequisites but do not implicitly add those maintenance operations; each requires separate requirements, safety analysis, and tasks.

The exact command tree and safety requirements are normative in `specs/kernel-composition/maintenance.md`; wire schemas, roots, locks, crash transitions, evidence rules, and command outcomes are normative in `specs/kernel-composition/maintenance-protocol-v1.md`. `docs/omegon-maintain.md` is their durable architecture summary.

Offline release verification has an explicit implementation prerequisite: the
selected verifier must support Sigstore bundle v0.3 and actually validate the
Fulcio chain, Rekor SET, Merkle inclusion proof, and signed checkpoint entirely
offline against compiled trust roots. The existing locked verifier supports only
older bundle profiles and omits those transparency checks, so mutation/audit
work lands as task 0.6a while verifier selection, trust material, and the signed
fixture matrix remain fail-closed task 0.6b. This split does not relax the
normative `release verify` contract.

Source, linked-development, and release-package launch paths are tested in this first slice rather than deferred to final packaging.

## Durable session authority

Before the shared supervisor becomes authoritative, define minimum append-only facts for:

- session creation and identity;
- prompt admission or rejection;
- queue insertion, removal, and ordering;
- turn start and generation identity;
- cancellation and revocation requests;
- invocation preparation and terminal settlement;
- turn completion, failure, cancellation, timeout, or unknown outcome.

The approved v1 vocabulary is `session.created`, `prompt.admitted`,
`prompt.rejected`, `prompt.removed`, `turn.started`,
`turn.interruption_requested`, `invocation.registered`,
`invocation.classified_unknown`, `invocation.settled`, and `turn.closed`.
Prompt admission and FIFO insertion are one transition; turn start atomically
promotes the queue head. Each event has a contiguous session-wide sequence and
immutable compatibility version. A snapshot records its stream, reducer, last
sequence, and last event identity and can reconstruct queue, active-turn,
cancellation, invocation, and terminal state without broadcast replay.

The append-only authority sidecar is distinct from current whole-file
conversation snapshots, metadata checkpoints, journals, and audit streams.
Legacy projections are not converted into fictional historical facts. Strict
replay rejects gaps, conflicts, invalid transitions, and unsupported authority
events or versions. The full envelope, reducer, recovery, and compatibility
contract is recorded in `docs/runtime-session-semantic-protocol.md`.

One supervisor implementation is instantiated per session. Cross-session hosts may own many supervisors, but TUI, ACP, daemon, Web/IPC, and bounded ingress submit to the owning session instance. Frontend busy and streaming state is a projection of supervisor state.

The default loop is a policy driver. It proposes typed step, message, invocation, continuation, and terminal intents. The kernel session state machine validates and commits each transition exactly once. The loop does not independently mutate canonical session truth or publish terminal completion.

Slice 1 registers invocation identity and conservatively classifies registered
unsettled invocations as unknown after runtime loss. Slice 3 owns durable
prepared/dispatched/acknowledged lease states and safe late settlement. Slice 5
adds complete model-context, route, assistant, tool, step, and compaction facts;
neither later slice redefines Slice-1 sequencing or closure semantics.

## Contribution lifecycle

### Static contributions

Statically linked contributions publish declarations before activation. The candidate graph validates IDs, invocation bindings, owner tier, dependencies, conflicts, platform support, effects, protocol ranges, and transition policy. Duplicate or ambiguous ownership is fatal unless an explicit replacement declaration names the superseded owner.

### Dynamic contributions

Dynamic protocols use two admission stages:

1. **Trust admission:** static preflight identifies code, protocol range, minimum dependencies, requested trust, and probe requirements. Probe code may execute only after explicit trusted-code admission or verified confinement.
2. **Capability admission:** the host starts an admitted probe in quarantine without brokered host-effect leases, negotiates a frozen declaration set, validates the graph, waits for readiness, and atomically promotes the candidate generation.

Heartbeat loss, startup timeout, crash loops, dependency degradation, restart/backoff, drain deadlines, forced cleanup, and quarantine are typed lifecycle outcomes. Required dynamic contributions must expose minimum dependency and trust requirements statically.

### Generations

Slice 2 binds registrations and candidate resources to one immutable composition generation. Candidate failure leaves the previous generation callable and removes all candidate resources. A graph-derived compatibility adapter feeds the legacy EventBus path, so registration order cannot select an owner rejected by the graph. Generation-bound invocation leases, stale-call denial, and privileged dispatch migration remain Slice 3. Model-visible schemas change only at turn-safe promotion boundaries.

The composition generation is distinct from a process or agent instance ID.
New sessions capture the active composition generation. Existing Slice-1 values
remain valid opaque legacy generation identifiers. Slice 2 does not add live
session migration; that requires a separately specified durable quiescent
migration event.

### Slice 2 implementation boundary

Slice 2 owns composition discovery, pure graph validation, activation
eligibility, candidate generation construction, readiness, atomic publication,
health, drain, retirement, and shared diagnostics. It does not own privileged
invocation leases or replace the legacy dispatch engine.

Concrete ownership for Slice 2 is:

- renderer-neutral declaration and diagnostic contracts in `omegon-traits`;
- the pure graph builder and generation/lifecycle owner in
  `core/crates/omegon/src/contribution_graph.rs`;
- phased discovery and activation orchestration in `setup.rs`;
- a one-way graph-to-legacy compatibility adapter in `bus.rs`;
- transport-specific static preflight and quarantine adapters in existing
  extension and plugin/MCP owners;
- one semantic diagnostic projection under `surfaces/` for supported clients.

The first red-test matrix must prove deterministic all-error diagnostics for
duplicates, ambiguous bindings, cycles, missing owners, conflicts, and protocol
incompatibility; no dynamic spawn before trust/confinement admission; no
candidate registration publication before readiness; previous-generation
survival after candidate failure; no registration-order owner selection; no
active-session generation drift; and honest cleanup state across unowned
boundaries.

## Admission and invocation

The kernel invocation path covers model tools, operator actions, trust-boundary calls, calls consuming caller authority, durable mutations, and host-effect-bearing internal calls. Pure in-process computation and read-only domain queries may use typed service handles directly.

The path is:

```text
resolve owner and generation
  -> combine admission policy
  -> persist Prepared and issue lease
  -> persist Dispatched
  -> hand request to owner with stable call/deduplication ID
  -> persist Acknowledged when authoritative acknowledgement arrives
  -> persist Settled or Unknown
  -> close lease exactly once
```

`Dispatched` is durable before transport handoff. Every unsettled `Dispatched` invocation recovers as unknown completion. Mutating calls are not retried unless the owner contract enforces idempotency or deduplication for the stable call ID. Failure to persist settlement fences further mutation and records emergency recovery evidence; it does not report ordinary completion.

Admission is monotonic. Policy sources can narrow authority, but no downstream adapter can widen a denial. Unknown owners, capabilities, effects, schemas, or provenance receive no privileged lease. Declarations request effects; they do not prove confinement.

Slice 3.1 establishes the authoritative in-memory seam for model-tool calls: accepted-graph resolution, caller/surface scope, the current name-based RBAC ceiling, layered permission policy, operator approval, lease issuance, immediate generation/owner revalidation, owner handoff, and exactly-once in-memory closure. The lease captures composition and owner generations, capability, principal, call, scope, admitted declaration effects, and transition policy. Unknown or incompletely declared invocations receive no lease. A publication that changes the composition generation makes an undispatched lease stale; rejected candidates preserve the prior generation and do not revoke it.

This first lane does not claim crash consistency. Slice 3.2 replaces compatibility name-derived RBAC, effects, timeout, parallelism, retry, and transaction metadata with declaration authority for leased model-tool calls. Capability declarations now carry typed principal eligibility, bounded timeout class, retry/idempotency/deduplication policy, serial or parallel-safe scheduling, and explicit independent-mutation or best-effort-rollback behavior. Validation fails closed on missing principals, zero attempts, unsafe non-idempotent retry, parallel rollback, and effect/transaction contradictions. The lease captures and dispatch revalidates the complete policy; RBAC derives from declared effects, caller timeout arguments can only narrow the declared class ceiling, and the scheduler no longer uses a tool-name parallel allowlist or behavioral mutation catalog as authority.

Static feature and legacy provider hooks may publish precise tool policy; absent hooks receive a conservative host-authored adapter, and external tools retain the full host-effect envelope rather than fabricated least authority. Retry metadata is captured but does not authorize crash or unknown-completion replay.

Slice 3.3 persists `invocation.prepared` after admission and approval but before materializing an executable lease, then persists `invocation.dispatched` after exactly-once lease claim and generation revalidation but before host or local owner handoff. Preparation captures lease and invocation IDs, provider-visible call ID, optional owner-enforced deduplication ID, turn, principal, capability and owner generations, effects, execution policy, transition policy, and surfaces. The same invocation metadata reaches local provider, extension, MCP, and host-delegation adapters; unsupported transports do not claim deduplication. Interactive, ACP, daemon, and bounded turns share the session authority writer and exact durable turn scope, while explicit no-session compatibility calls remain ephemeral. Durable edit calls execute one visible call per owner handoff instead of collapsing several call identities into a hidden batch.

`Prepared` and `Dispatched` are distinct from legacy `invocation.registered`, whose conservative recovery behavior is preserved for old streams. Slice 3.3 recovery retains new prepared and dispatched states without inventing acknowledgement, settlement, or unknown completion; those transitions begin in Slice 3.4. A failed preparation writes no lease, and a failed dispatch append revokes the claimed lease before owner entry. The append-only JSONL fact remains authoritative if the replaceable snapshot cache update fails. Existing direct EventBus/internal/operator paths remain named compatibility paths until Slice 3.7; they are not silently assigned fabricated principals or administrator leases. Reactive path grants and extension HostAction approvals remain narrower post-lease guards and cannot override an upstream denial.

Slice 3.4 persists owner acknowledgement and typed terminal settlement before ordinary completion or lease closure. Unsettled dispatched and acknowledged calls recover as unknown completion, while prepared calls remain explicitly unhanded-off. Live external transport ambiguity uses the same unknown classification and is not automatically replayed.

Slice 3.5 requires every mutating execution declaration to carry a validated durable mutation domain and fence key. A post-dispatch acknowledgement, unknown-classification, or settlement durability failure writes strict append-only emergency evidence through a directory shared by session authorities and independent of the failed authority JSONL. Evidence binds call, capability, owner and composition generations, invocation, lease, session, turn, fence, and failure phase. Matching mutation is denied immediately before preparation; malformed evidence and emergency-writer failure fail closed. No ordinary runtime path removes a fence, and the current maintenance companion exposes no clearing command without a later deterministic reconciliation or explicit audited recovery contract.

Slice 3.6 checks stable call identity against unknown invocations across the complete session before preparing another lease. A mutating unknown is unsafe unless its original persisted execution contract was idempotent or carried owner-enforced deduplication for that exact call ID; a current replacement declaration cannot retroactively make the prior handoff safe, and legacy unknown records fail closed. Unsafe replay receives a typed denial before another `Prepared` fact. Ambiguous ACP host-write responses no longer trigger a second local write. This slice does not enable safe replay, relax duplicate-call invariants, or introduce attempt lineage/request fingerprints; provider-request retries remain separate because they precede completed tool-call dispatch.

Slice 3.7 makes admission and dispatch kind-aware rather than tool-only. Graph-registered feature commands invoked through TUI, CLI remote execution, ACP, Web, or IPC carry explicit operator principals and owner-declared surfaces through generation revalidation, acknowledgement, settlement, and exactly-once lease closure. Model-loop path grants invoke the graph-declared `trust_directory` internal owner under an internal principal while retaining the parent session and turn authority. Automatic memory ingestion and host-mediated persona/tone switches use explicit internal bindings and leases; model-facing memory mutations declare state-changing effects instead of read-only orientation. Managed-delegation tools declare both model and service principals plus Model/Web/Daemon surfaces, and authenticated supervisor calls use service leases instead of direct EventBus dispatch; `agents_status` resolves to the owned delegate-status tool rather than an unowned name. Operator context-pack requests call the shared context provider through a typed read-only service handle rather than impersonating a tool invocation. The extension-provided `voice_session_stop` tool declares model and TUI surfaces plus service authority, and the interactive coordinator invokes it under the promoted turn's real authority. Daemon vox polling invokes the declared `vox_route` tool under an ephemeral Service/Daemon lease and projects its result into the existing daemon event envelope. Arbitrary ACP methods share one extension-owned conservative transport capability because the current protocol does not declare per-method effects; the ACP thread tracks availability, while the worker-owned EventBus admits, revalidates, acknowledges, dispatches, settles, and classifies ambiguous transport loss. Lease-less imperative `actions/execute` requests fail closed before host execution, and an operator approval only contributes the operator gate instead of fabricating project, runtime, or trusted-origin authority. Declarative native HostActions and MCP review candidates receive a host-only parent guard after lease revalidation; it requires the parent to remain dispatching, verifies conservative HostAction effects are contained by the parent declaration, and consumes each child dispatch identity once. These migrations do not fabricate durability for idle or post-loop calls: without an active authority turn they remain explicit ephemeral leases.

## Process ownership

Every process, task, socket, listener, subscription, temporary file, and durable writer has one host-recorded owner and generation. Complete tree settlement is required only inside a lifecycle boundary Omegon can own. Cross-boundary processes, including Windows-host executables launched from WSL, settle as degraded or unverified; profiles requiring strict cleanup reject those transports.

## Semantic event spine

The minimum supervisor facts expand into a complete semantic session event contract containing admitted input, model-visible context provenance, provider route and schema generation, assistant output, tool calls/results, invocation states, step/turn boundaries, and cancellation/interruption evidence.

Slice 5.1 uses one durable step for each internal loop iteration. A step may contain
multiple ordered model requests only when the same iteration repairs
context-overflow or provider-history state; every repaired request receives a new
request identity and route lease while retaining the step identity. The existing
`route.lease_recorded` v1 payload remains immutable. A separate
`model.request_route_joined` fact binds that lease to the request and step.

Model-visible context is recorded as an ordered manifest of immutable content
references and provenance. The tool schema set has a content-addressed identity
over its canonical ordered composition plus the composition and owner generations
that produced it. Assistant display content is durably appended as ordered,
bounded, coalesced chunks before broadcast; durability is per bounded append, not
per provider token. Hidden reasoning and opaque provider continuity are persisted
only when a provider requires them for continuation, in restricted
content-addressed blobs excluded from default projections. A captured
generation-bound provider policy is either `none` or `restricted_required` with
declared kinds and a size ceiling; it does not widen `route.lease_recorded` v1.
Arbitrary raw provider payloads are never authority facts or continuity blobs.
Transport retries under one joined request carry contiguous response-attempt
ordinals on chunks, continuity, message commit, and request closure. Failed
attempt chunks remain canonical but cannot enter another attempt's commit; each
failed attempt is durably identified and terminalized before a later attempt.

Slice 5.1 emits these facts only for authority-backed sessions. It makes denied
tool calls and their denied results canonical, links admitted calls and terminal
results to existing invocation facts, and adds deterministic request/step
abandonment sufficient to recover a crashed writer. It does not add tool-progress
facts. Complete replay policy, consumer migration, compaction derivation,
sessionless semantic streams, and lagged/corrupt consumer fixtures remain tasks
5.2 through 5.5.

Current storage is plural input to migration:

- resumable whole-file snapshots persist an LLM-facing projection;
- checkpoint JSONL persists metadata rather than semantic replay facts;
- the agent journal is a human narrative;
- audit streams have separate schemas and authority.

Storage backends, provider-history projection, transcript rendering, compaction, and snapshots derive from the semantic record. No current stream is silently reclassified as canonical without explicit conversion and compatibility tests.

## Migration strategy

1. Build and release-test the independent maintenance artifact.
2. Define minimum durable session facts and promote one supervisor per session.
3. Evolve authority-neutral capability declarations into the pre-activation contribution graph.
4. Extract admission, leases, scheduling, progress, terminalization, and retry classification into one invocation pipeline.
5. Unify provider contribution metadata and reduce the loop to a transition-intent driver.
6. Complete the semantic event spine and migrate projections/storage.
7. Extract optional domains and unify external contribution discovery.
8. Complete artifact separation, contribution locks, and composition budgets.

Each step retains adapters for current behavior until parity and absence tests pass. Crate movement without authority, lifecycle, or failure-isolation change does not satisfy a migration task.

## Documentation co-delivery

Documentation is part of each implementation lane, not a final release activity. Before mutation, every lane classifies:

- the durable architecture/developer documents it owns;
- serialized compatibility or migration notes;
- operator-visible command, configuration, output, permission, recovery, packaging, or availability changes;
- affected public pages under `site/src/pages/docs/`;
- affected canonical command examples under `site/snippets/`;
- the narrowest documentation and site validation commands.

A lane cannot pass its exit gate until implemented behavior, source design, OpenSpec requirements/tasks, durable docs, public site pages, snippets, CLI help, and operator terminology agree. A lane with no public delta records that decision explicitly. Later groups may refine earlier documentation when authority migrates, but they may not serve as a deferred documentation bucket for behavior already shipped.

## Safety invariants

- Trust admission precedes execution of dynamic probe code.
- Requested confinement fails closed when the required boundary is unavailable.
- Unknown effects and capability owners fail closed.
- Mutating unknown-completion calls do not retry without owner-enforced deduplication.
- Frontends can expose less authority than the runtime, never more.
- No optional contribution or normal integration startup path is required to diagnose, deny, or quarantine that contribution.
- Durable facts and reconstructable snapshots precede advisory broadcasts.
- Strict cleanup claims are limited to process trees inside the host ownership boundary.
- Documentation and applicable public site/snippet changes pass within the same lane as their behavior.

## Slice-1 design gate

The minimum semantic vocabulary, serialized evolution policy, strict replay
rules, snapshot reconstruction contract, deterministic interruption closure,
and Slice 1/3/5 ownership split are approved in
`docs/runtime-session-semantic-protocol.md` and the runtime-session authority
spec. Implementation proceeds with an adjacent authority sidecar and must not
weaken the stronger stale-interrupt and exactly-once settlement behavior in the
currently compiled interactive coordinator.

## Slice-4 design gate

Slice 4 separates provider declaration from request routing. Task 4.1 adds and
validates provider contributions; it does not claim one cross-host route
authority or durable request leases. Task 4.2 consumes those contributions in a
single route service and records the minimum route lease before provider
dispatch. Slice 5 later extends the semantic event spine and its projections; it
does not postpone the route identity required to explain a Slice-4 dispatch.

Provider contribution metadata is owned in the `omegon` integration crate so a
bridge-factory binding can resolve the crate-local `LlmBridge` contract without
pulling provider transports or secrets into `omegon-traits`. Existing
`RuntimeContributionId` and generation IDs remain the owner vocabulary. The
runtime inference inventory remains the authority for layered endpoint,
offering, modality, capability, and provenance evidence; a provider contribution
binds to that authority rather than duplicating model rows. Executable factory
bindings are host-local typed identities, not serialized code pointers.

Provider identity, endpoint/deployment identity, model offering, conceptual
model identity, and credential source remain distinct. Authentication class
describes accepted credential mechanisms, not merely login UI. Tool-schema
dialect describes the executable adapter behavior and may explicitly declare
tools unsupported. Fallback compatibility is directed, model-family-bounded,
and non-transitive. OpenAI-compatible wire support alone never establishes
fallback eligibility; current route policy and admission may only narrow a
declared relation.

Compatibility adapters may retain current provider constructors and host-local
route callers through task 4.1, but every executable provider must resolve to
one complete contribution independent of registration order. Task 4.2 removes
host-local bridge/fallback construction across interactive, daemon, ACP, child,
and bounded execution. Selecting another route between requests is distinct
from replacing the route-service implementation; the latter remains boot- or
quiescent-boundary only.

The task-4.1 red-test matrix covers duplicate provider IDs and aliases, missing
inventory/auth/schema/factory/evidence semantics, unsupported tool contracts,
dangling or wire-only fallback relations, stable ordering, and parity between
legacy constructible providers and the validated registry. OpenAI/Codex and
Google/Antigravity relations remain explicit and model-bounded; unrelated
providers remain isolated. Task 4.2 adds cross-host route parity, selected versus
serving identity, stale generation, and pre-dispatch durability tests.

Task 4.1 updates the owning provider/decomposition developer docs and release
notes. Public model-selection, authentication, fallback, and route-evidence
guidance changes only when operator-visible routing consumes the declarations;
if command syntax remains unchanged, canonical snippets remain unchanged and
that decision is recorded in the task.

Task 4.2 implements the request boundary in `ProviderRouteService`. Existing
provider constructors remain transport factories behind that service, while
compatibility `LlmBridge` handles carry immutable selected and serving route
hints instead of erasing fallback identity. The common loop dispatch path,
compaction, smoke execution, and lightweight completion record a lease before
calling `stream`; daemon, ACP, child, bounded, and interactive hosts inherit the
same path rather than constructing fallback chains.

Session authority accepts `route.lease_recorded` only for its active turn and
reduces leases by immutable lease identity. Sessionless inference writes the same
versioned lease payload to an explicit step-owned durable JSONL stream under the
runtime home. A partially populated session scope is not downgraded to a step.
Current contribution generation and directed fallback compatibility are
revalidated immediately before persistence, and persistence failure blocks the
provider call. Full context, assistant stream/result, and projection events
remain Slice 5 work rather than being folded into this minimum dispatch fact.

Task 4.3 introduces a single compiled `ReleaseCoupledLoopDriver` with four
required trait contracts: session, leased route, context assembly, and privileged
invocation. Compatibility adapters remain crate-local so this boundary does not
move provider transports, mutable conversation state, feature composition, or
context implementation into shared protocol crates. Every compiled host uses the
same turn constructor; no optional port or raw-loop host entry point remains.

The session contract captures the invocation principal, session/turn authority,
and ephemeral route-step identity. Driver admission rejects partially populated
authority, authority from another session, and a turn other than the authority's
active turn. The route contract captures the controller and bridge together and
requires coherent serving identity; only an explicitly disconnected bridge may
omit it. The loop implementation consumes those captured contracts rather than
reading route or invocation authority directly from policy configuration.

Normal driver termination produces one typed terminal proposal for completed,
provider-exhausted, or failed execution. Interactive, ACP, daemon, and bounded
hosts submit that proposal through the existing supervisor only after their
owned cleanup, preserving cancellation/timeout narrowing and canonical
authority ordering. Advisory completion broadcasts remain projections. Task 4.4
removes concrete implementation and feature names from policy behind these
contracts; task 4.5 governs replacement at boot or quiescence; Slice 5 adds the
message, context, assistant, tool-result, continuation, and step facts that do
not yet exist in the minimum authority vocabulary.

Task 4.4 keeps the compatibility implementations release-coupled but removes
their concrete names and authority inputs from production loop policy. The
driver captures one opaque host binding and constructs the four required ports;
provider route policy is snapshotted by the route port instead of calling back
into `LoopConfig`. Tool admission, batching, owner handoff, permission and
operator-wait presentation, memory/lifecycle requests, context implementation,
and plan/recovery/finalization policy remain behind their owning contracts.
Source guards cover concrete provider and transport names, direct invocation
lifecycle calls, tool-name batching policy, memory attribution, concrete context
implementation, frontend permission presentation, lifecycle finalization, plan
and completion policy, and adapter callbacks into loop orchestration. This does
not make the driver dynamically replaceable, establish a quiescent replacement
boundary, or add complete semantic step/message persistence.

Task 4.5 adds one session-lifetime execution owner around a selectable atomic
pair of loop-driver and provider-route-service handles and their validated
contribution-generation IDs. Durable turn start and pair capture share the
owner's coordination gate, so a turn receives either the complete prior pair or
the complete migrated pair. Sessionless execution retains the immutable boot
pair and cannot request migration.

A mid-turn replacement request stores only in-memory `Pending` intent and leaves
the active capture unchanged. Turn closure does not promote it, and starting a
later turn does not promote it. A deliberate caller must invoke
`commit_pending_at_quiescence`; only that operation may append the version-1
`session.execution_binding_migrated` fact and then publish the target pair.
Admission requires an idle session, the exact process-local and durable source,
and no registered, prepared, dispatched, acknowledged, legacy-unknown, or
durable-unknown invocation. The append is synced before in-memory publication,
and failure retains both the current pair and pending intent.

The reduced binding remains optional so legacy streams replay without invented
history. Resume establishes the selected boot binding process-locally and
appends nothing; it must match durable migration history when such history
exists. Interactive, ACP, daemon, bounded, headless/child, and Sentry execution
now use an owner capture, while smoke, compaction, and lightweight sessionless
routes use the immutable boot binding. Source guards reject direct driver/route
construction and supervisor auto-commit. This does not add Slice 5 message,
context, tool-result, continuation, or step facts; task 4.6 closes the separate
documentation lane below.

Task 4.6 closes Slice 4 by reconciling the canonical provider-contribution and
route-lease guide, related durable routing/session/recovery documents, public
model-selection, fallback, authentication, and route-evidence guidance, the
README, and release notes. Current documentation does not turn declaration or
inventory presence into execution, claim global failover, treat compatibility
as symmetric or transitive, present Antigravity as executable, claim Anthropic
subscription headless use is blocked, expose historical leases through
`/model`, make inventory diagnostics a dispatch gate, promise an exhaustive
current provider/model inventory, or infer a precise credential store from
authentication-class-only evidence. Command and configuration syntax did not
change, and canonical site snippets remain unchanged. Slice 5 semantic event
spine work is still explicitly outside this closeout.

## Slice-5 design gate

Task 5.0 freezes the production-emission contract in
`docs/runtime-session-semantic-protocol.md`. The new required v1 vocabulary is
`step.started`, `model.request_prepared`, `model.request_route_joined`,
`assistant.content_appended`, `assistant.message_committed`,
`provider.continuity_stored`, `tool.call_recorded`, `tool.result_recorded`,
`model.request_closed`, `step.closed`, and `step.abandoned`. Slice-1 envelope,
sequence, command idempotency, and `turn.closed` semantics remain unchanged, as
does `route.lease_recorded` v1.

One `loop.rs` internal iteration maps to exactly one step. Step ordinals are
contiguous within a turn. Request ordinals are contiguous within a step and may
advance without closing the step only for context-overflow/history repair. Each
request has exactly one prepared fact, one route join to a distinct route lease,
and one terminal request outcome. Context items, assistant chunks, tool calls,
and tool results use contiguous zero-based ordinals in their declared scope.
Normal step closure requires every request and canonical tool call terminal.
Recovery appends deterministic open-invocation classifications first, then
open-request abandonment, then `step.abandoned`, then the existing
`turn.closed(interrupted)`. Live abnormal hosts use the same ordering after owned
cleanup, with the strongest truthful request outcome and the existing narrowed
failed, timed-out, cancelled, revoked, or interrupted turn outcome.

Content bytes referenced by Slice-5.1 facts are stored in a session-adjacent,
content-addressed blob store rather than embedded in authority records. References bind digest algorithm/digest, media
type, byte length, storage class, and projection class. Reads are descriptor-
confined to the session store and verify all reference fields before decode.
Default projections may read ordinary display/model content but never restricted
continuity blobs. Restricted blobs require the captured provider-continuation
owner and exact session/request purpose; exports, transcripts, diagnostics,
memory, and arbitrary extensions receive no implicit access.

Task 5.1 owns only authority-backed production emission and the minimum
deterministic abandonment path necessary for crash safety. Red tests must cover
multi-request same-step repair, lease/request mismatch, noncanonical schema-set
identity, chunk coalescing and ordinal gaps, projection before append, restricted
blob leakage or digest substitution, denied calls, invocation mismatch,
duplicate terminal facts, EOF/cancellation outcomes, and repeated recovery.
Tasks 5.2 through 5.5 own complete compatibility/replay matrices, reducer-backed
provider history and transcripts, legacy consumer/storage migration, compaction
checkpoints, and lagged/disconnected/corrupt consumer recovery. Sessionless full
semantic streams require a separate future design. Task 5.6 completed final developer and applicable public recovery
documentation. Task 5.0 itself changes no command, configuration, site page,
canonical snippet, or runtime behavior, so it requires no Unreleased behavior
entry.

Task 5.1 is production-active for complete authority-backed scopes. Normal loop
paths emit and terminalize one step exactly once. Runtime loss and host abnormal
paths classify unresolved handed-off invocations, terminalize any open request
with its latest response-attempt identity, append deterministic reason-bound
UUIDv5 `step.abandoned`, and only then permit canonical turn closure after host
cleanup. Interactive cancellation cooperatively drains the same pinned execution
under the supervisor deadline; timeout abort drops its state/authority borrows
and joins its turn-scoped projection relay before abandonment. Detached native
HTTP transport may finish externally but has no remaining receiver or projection
capability, so that completion is explicitly unverified rather than falsely
settled. Partial scopes fail closed and sessionless execution emits no fabricated
semantic spine. This does not claim task-5.2 replay rules, task-5.3 projections,
task-5.4 storage migration, or task-5.5 consumer recovery.

Task 5.2.0 freezes those next contracts without changing runtime. Once a lineage
contains its first full-spine fact, every later eligible operation must use the
full spine; there is no downgrade, concurrent old writer, or negotiated minimum
reader level. Legacy streams remain legacy, mixed streams have an explicit
boundary and exact full-spine suffix, and full streams use semantic authority
from their first eligible operation. Older or incomplete readers fail closed.
Missing or tampered referenced blobs prevent full recovery; only diagnostic
projections may render an unavailable marker, never provider history or exact
exports. Legacy transcript bytes are not promoted into authority.

The refinement adds new required v1 names without changing any existing v1
payload: `context.source_materialized`, `model.response_attempt_failed`,
`compaction.started`, `compaction.request_prepared`,
`compaction.response_attempt_failed`, `compaction.request_closed`,
`compaction.summary_committed`, `compaction.applied`, and
`compaction.abandoned`. Turn-owned compaction binds an
open turn/step and the unchanged route lease. Manual idle compaction instead
uses a session-scoped compaction identity and embedded route evidence; it does
not invent a prompt, turn, step, or `route.lease_recorded` payload. Idle
compaction holds the supervisor admission gate and publishes one atomic context
replacement only after its applied fact is durable. Sessionless work remains
route-only.

Task 5.2 implements strict event/reducer compatibility, response-attempt
validation, read-only prefix replay, reducer/cache v5, and the generic projector
cursor v1 contract. It does not switch provider history, transcripts, frontend
snapshots, or compaction checkpoints to those reducers. It also emits and reduces
the frozen provenance and compaction facts, recovers compaction before ordinary
turn terminalization, and atomically replaces idle host sessions only after
strict target validation. Task 5.3 derives the concrete projections with
output-before-cursor publication; it does not re-emit semantic authority. Task
5.4 now migrates the frozen storage and consumer set while preserving plural
authority and exact-frontier rules; sessionless semantic lineage remains
deferred rather than synthesizing history. Task 5.5 exercises
the frozen canonical corpus under lag, restart, disconnect, truncation,
corruption, and blob-loss conditions. Task 5.6 completes compatibility
publication and applicable public/developer documentation closeout.

Task 5.3.0 freezes the concrete derivation boundary before task 5.3 mutates
runtime projection code. Four internal semantic projectors, all projector
version 1 and schema version 1, are fixed as `session.provider-history`,
`session.transcript`, `session.frontend-snapshot`, and
`session.compaction-checkpoint`. Provider history is an immutable sequence of
exact joined-request inputs and can never be treated as synthesized input for a
later request. The normal transcript contains committed prompt, assistant, and
tool-result messages only. A frontend evidence snapshot may additionally show
durable uncommitted or abandoned assistant chunks, queue, active-turn, context,
and semantic-conversation state; downstream live tool progress remains an
ephemeral overlay rather than durable projection content. Committed content
followed by abandonment remains visible and carries abnormal status.

Every output uses the frozen availability envelope. A full lineage may claim
`exact_full`; a mixed lineage may claim only `exact_suffix` beginning at its
first full-spine fact and must mark full-session export unavailable; a legacy
lineage publishes an availability envelope with no content or exactness claim.
Restricted continuity is metadata-only where needed to prove request input and
is never dereferenced or serialized. Provider-history and transcript bodies use
immutable bounded chunks plus a bounded manifest that remains below the generic
16 MiB cursor-output ceiling. Frontend and compaction checkpoint each publish
one bounded output. RFC 8785 canonical JSON, semantic-key ordering, SHA-256, and
source event frontiers make byte output independent of wakeup batching.

One session publication coordinator captures the latest stable read-only replay
frontier, coalesces redundant wakeups, runs all four projectors against that
same frontier, and independently performs chunk-before-manifest and
output-before-cursor publication. A failed projector retains its previous
committed frontier without blocking the other three; no partial or unavailable
content is mislabeled exact. Projector-owned chunks are immutable and retained
for the authority lineage lifetime, while unreferenced temporary files may be
removed only under the projector lock. Task 5.3 remains shadow-only: it creates
and verifies these internal outputs but does not switch `ConversationState`,
provider dispatch, transcript commands, TUI, ACP, Web, IPC, whole-file
snapshots, or compaction compatibility consumers. Those switches remain task
5.4. The refinement itself is planning-only and therefore has no public docs,
site, command/configuration/snippet, or changelog behavior impact.

Task 5.3 implements that boundary with one capacity-one, dirty-bit worker owned
by each authority-backed runtime supervisor. Durable appends signal only after
sync; ordinary bursts coalesce for 50 ms with a 250 ms ceiling, while frozen
terminal, explicit flush, startup/recovery, and shutdown boundaries publish
immediately. The worker strict-replays end-of-stream before each run and records
typed replay, coordinator, and per-projector failures without entering the
authority append path. Session replacement clears and stops the old worker,
fences its session-specific adjacent root, and transfers its join handle to the
new supervisor for owned reaping without delaying host publication. Sessionless supervisors create no worker. Task 5.4 has now replaced the shadow-only consumer boundary with validated readers and source guards against direct projection-storage access.

Task 5.4.0 froze the consumer migration before task 5.4 changed runtime. The
authority-role matrix is intentionally plural: the semantic stream owns replay
facts and exact committed transcript; a synchronous immutable current-context
reducer owns provider-dispatch input at the captured authority frontier;
versioned host-state checkpoints own `IntentDocument` and plan state; an
append-only observation ledger owns operator observations; operator metadata owns
friendly name and description; semantic projections derive counters, catalog,
telemetry, frontend evidence, and compaction state; audit and Markdown journal
remain separate diagnostic/narrative records. No projector, compatibility
snapshot, journal, audit row, or observation may be promoted into semantic
authority.

Provider dispatch never waits for or reads provider-history publication. Under
the session admission/writer coordination boundary it captures the latest
durable frontier and synchronously reduces bounded, provenance-complete model
context into `CurrentContextViewV1`; a gap, unsupported fact, missing blob,
unattributed post-boundary item, bound violation, or frontier mismatch fails
before dispatch. Exact resume uses the same law. Frontends may render a validated
older snapshot only with its disclosed cursor and lag; no stale snapshot can
authorize dispatch or an exact-resume claim.

Full lineages resume and export exactly from semantic state. Mixed lineages may
resume only as an explicitly labeled compatibility legacy base followed by the
exact semantic suffix; the base never acquires historical semantic authority and exact
full-session export stays unavailable. Web historical output for mixed lineages
is the exact suffix only. Legacy sessions retain labeled compatibility resume;
sessionless semantic lineage remains deferred. `/transcript` is reassigned to
the exact committed semantic transcript, while `/session-export` is the distinct
presentation/evidence export name.

The new host-state, observation, catalog, telemetry, audit-source, and journal-
provenance schemas start at version 1 without changing authority event v1,
projection schema v1, reducer/cache v5, or legacy file shapes. Slice 5.6 stops
rewriting `.json`/`.meta.json` for full and materialized-mixed sessions; legacy
and not-yet-materialized mixed sessions retain the pair only as a one-way import
source. The semantic writer and forward-only lineage remain active, and no old
writer may append a reduced event set. Task 5.4 implements the consumer cutover,
task 5.5 supplies adverse-consumer execution, and task 5.6 completes
compatibility publication plus applicable public documentation.

Task 5.5.0 freezes that adverse-consumer execution without changing production.
The private semantic protocol is the normative campaign: 54 stable scenario IDs
form a pairwise covering array across lineage (`legacy`, `mixed`, `full`),
lifecycle (late attach, lag, disconnect, restart, replacement, and steady
publication), and consumer class (exact, projection, frontend, host record,
evidence, and compatibility mirror). Faults and dispositions are closed typed
vocabularies, and each consumer class has one fail-closed, explicitly degraded,
or best-effort law. Tests use copied fixture sandboxes and deterministic
append/sync/rename/read, notification, worker, and replacement barriers rather
than sleeps or mutation of the checked corpus. Required CI runs on Linux,
macOS, and Windows and must complete within 15 seconds per platform; broader
filesystem crash probes remain non-blocking evidence.

The freeze resolves the remaining policy choices. A projector may quarantine a
proven corrupt chunk and deterministically republish it from validated authority
only while holding that projector's lock; no other store may be silently
repaired or quarantined. Session replacement validates authority, blobs,
host-state, observations, and catalog identity, but derived projection damage or
absence is disclosed and does not prevent publication while the replacement
worker rebuilds. ACP worker/supervisor completion and authoritative idle queue
state release local busy gates even when `AgentEnd` or equivalent advisory
notification is skipped; notification draining is bounded. IPC lag enqueues one
reconciled current state automatically before later deltas. A missing observation
ledger degrades open only when no durable marker, host frontier, catalog field,
or mirror provenance says it existed; malformed or torn bytes fail closed.
Malformed semantic audit input stops the semantic audit cursor and warns, with
no silently generated duplicate or quarantine row. Journal failure to read an
existing authority is `semantic_source_unavailable`, not sessionless. Existing
authority with no catalog record is a fatal store-set invariant. A durable
semantic save followed by mirror failure returns typed partial publication and
remains semantically resumable.

The current corpus has authority/reducer vectors but not the frozen cross-
consumer cases for corrupt projector chunks, damaged-projection replacement,
skipped ACP notifications, automatic IPC lag reconciliation, ledger existence
proof, malformed semantic audit input, journal authority unavailability,
missing catalog identity, partial mirror publication, or platform-specific
atomic publication. These are exact red gaps, not permission to revise accepted
vectors. Task 5.5 may correct behavior exposed by those fixtures while retaining
event v1, reducer/cache v5, cursor v1, projection v1, and task-5.4 store schemas.
Task 5.6 is the only compatibility-publication-removal and
developer/applicable-public-doc boundary. Its refinement defines semantic
self-sufficiency narrowly: full lineage, or mixed lineage carrying exactly one
durable content-addressed legacy compatibility base. Those sessions stop
rewriting `.json`/`.meta.json`; legacy and not-yet-materialized mixed sessions
retain the pair only as a one-way import source. Opening a valid legacy pair
beside pre-boundary authority materializes that base before the first full-spine
step and fixes mixed lineage. Existing pair artifacts are not automatically
deleted, but stale or missing pair bytes cannot affect a self-sufficient
session. Maintenance becomes catalog-first for inventory, inspection, and
quarantine, with pair fallback only for legacy import. Closeout does not invent
the previously described rollback consumer switch: no such runtime selector
exists. Event v1, reducer/cache v5, cursor v1, projection v1, and task-5.4 host
schemas remain unchanged. The refinement itself was planning-only; task 5.6 now
implements this boundary and its documentation without changing those frozen
schemas.
