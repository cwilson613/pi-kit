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

This change remains `proposed` overall. Slices 0, 1, and 2 are complete. Homebrew publication is not a Slice-zero exit gate; existing formula verification remains a best-effort packaging safeguard. Pairwise install, self-update, and version switching publish immutable complete generations and select the executable pair plus receipt through one atomic activation link. Each later slice begins with an explicit refinement gate that names concrete ownership, compatibility boundaries, red tests, and documentation impact before production mutation.

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

Slice 3.7 begins by making admission and dispatch kind-aware rather than tool-only. Graph-registered feature commands invoked through TUI, CLI remote execution, or ACP now carry explicit operator principals and declared surfaces through generation revalidation, acknowledgement, settlement, and exactly-once lease closure. Control forwarding retains non-TUI surface provenance; TUI and CLI feature-command bridges use the lease path, while Web and IPC remain explicit compatibility dispatch until declarations authorize those surfaces. Model-loop path grants invoke the graph-declared `trust_directory` internal owner under an internal principal while retaining the parent session and turn authority. Automatic memory ingestion and host-mediated persona/tone switches use explicit internal bindings and leases; model-facing memory mutations now declare state-changing effects instead of read-only orientation. These migrations do not fabricate durability for idle or post-loop calls: without an active authority turn they remain explicit ephemeral leases. Read-only context requests still need a typed service handle or precise read-only internal declaration, service-triggered delegate tools need truthful service principal/surface declarations, and arbitrary extension polling RPC plus nested extension/MCP HostActions remain compatibility paths.

## Process ownership

Every process, task, socket, listener, subscription, temporary file, and durable writer has one host-recorded owner and generation. Complete tree settlement is required only inside a lifecycle boundary Omegon can own. Cross-boundary processes, including Windows-host executables launched from WSL, settle as degraded or unverified; profiles requiring strict cleanup reject those transports.

## Semantic event spine

The minimum supervisor facts expand into a complete semantic session event contract containing admitted input, model-visible context provenance, provider route and schema generation, assistant output, tool calls/results, invocation states, step/turn boundaries, and cancellation/interruption evidence.

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
