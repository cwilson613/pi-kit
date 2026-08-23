# Runtime session: durable authority - Delta Spec

## ADDED Requirements

### Requirement: Every session has one authoritative supervisor

The runtime must instantiate one supervisor implementation per session. All ingress paths for that session must submit prompt, queue, cancellation, and terminal intents to the owning supervisor rather than maintaining independent active-turn truth.

#### Scenario: Two frontends observe one session
Given TUI and ACP adapters are attached to the same session
When one adapter submits a prompt
Then both adapters project the same supervisor-owned queue and active-turn identity
And neither adapter can independently admit a second active turn

#### Scenario: Terminal event is missed
Given a frontend misses an advisory terminal event
When it receives a later authoritative snapshot and cursor
Then it clears local busy state from the supervisor's terminal outcome
And it can submit the next turn without waiting for replay of the missed broadcast

### Requirement: Minimum session authority is durable before projection

Prompt admission, queue mutation, turn start, cancellation/revocation request, invocation identity, and terminal closure must be represented as ordered, versioned semantic facts before corresponding live snapshots or notifications are published.

The required Slice-1 v1 vocabulary is exactly `session.created`, `prompt.admitted`, `prompt.rejected`, `prompt.removed`, `turn.started`, `turn.interruption_requested`, `invocation.registered`, `invocation.classified_unknown`, `invocation.settled`, and `turn.closed`. Prompt admission atomically inserts into a FIFO queue; turn start atomically removes the selected queue head and makes it active. Slice 1 has no arbitrary queue reorder event. Registered but unsettled invocations are conservatively unknown after runtime loss; Slice 3 adds authoritative prepared/dispatched/acknowledged lease states without redefining these minimum identities.

`turn.closed` outcomes are `completed`, `failed`, `cancelled`, `timed_out`, `revoked`, `interrupted`, or `unknown`. `completed` means ordinary successful completion rather than merely terminal. Cancellation request does not imply closure, and `cancelled` may be committed only after bounded cleanup inside Omegon's ownership boundary.

#### Scenario: Runtime restarts during an active turn
Given durable facts include an admitted prompt and turn start without terminal closure
When the session is recovered
Then recovery appends at most one terminal `interrupted` closure under a deterministic recovery identity before publishing the recovered snapshot
And the closure does not imply success or failure for any unsettled invocation
And each dispatched but unsettled invocation remains `unknown` until authoritative owner evidence safely settles it

#### Scenario: Recovery repeats
Given recovery already appended the deterministic interrupted closure
When the same session is recovered again without new authoritative evidence
Then no second terminal closure is appended
And unresolved invocation classifications do not change

#### Scenario: Snapshot is reconstructed
Given a semantic event sequence and a stored cursor
When the runtime rebuilds a session snapshot
Then queue order, active turn, cancellation state, and terminal outcome equal the state obtained by applying each event once in sequence

### Requirement: Session facts have one strict serialized compatibility contract

Every authority event envelope must contain envelope version, immutable event/session/stream identities, contiguous sequence, stable dotted event type, event version, command ID and fingerprint, optional causation event ID, diagnostic UTC timestamp, and event-specific payload. Session IDs remain canonical opaque Omegon IDs; stream, command, prompt, turn, interruption, invocation, and event IDs are stable lowercase UUIDs rather than process-local counters.

Sequence starts at one with `session.created`, is contiguous, and cannot be reused or renumbered. The same command ID and fingerprint is idempotent; conflicting command-ID reuse is refused. A record is flushed and synced before the authoritative snapshot advances, a notification is published, or command acceptance is returned.

Envelope and payload decoding is strict. Missing, duplicate, or unknown fields, invalid enums, unknown envelope/event versions, sequence gaps, conflicting IDs, and invalid reducer transitions prevent authoritative recovery. Slice-1 authority events are all required state and cannot be silently skipped. Persisted snapshots are replaceable caches bound to session ID, stream ID, reducer version, last sequence, and last event ID; the append-only event stream remains authoritative.

#### Scenario: Append fails before durability
Given a supervisor transition is valid in memory
When its authority fact cannot be fully appended and synced
Then the authoritative snapshot remains at the prior cursor
And no corresponding advisory event or accepted command result is published

#### Scenario: Cached snapshot does not match its stream cursor
Given a cached snapshot names a sequence or event identity not present at the same stream cursor
When recovery starts
Then the cache is discarded and replay starts from sequence one
And the authority stream is not rewritten to match the cache

#### Scenario: Legacy session has no authority stream
Given an existing whole-file conversation snapshot predates the semantic authority protocol
When durable authority is enabled for that session
Then a new explicit stream lineage begins at the migration boundary
And the runtime does not synthesize fictional historical facts from the conversation projection

### Requirement: The loop proposes transitions but does not own session truth

The selected loop driver must submit typed message, step, invocation, continuation, and terminal intents to the kernel session state machine. Only the state machine may validate and durably commit canonical transitions or terminal completion.

#### Scenario: Loop proposes completion twice
Given a turn already has a committed terminal outcome
When the loop or a delayed provider event proposes another completion
Then the state machine rejects or idempotently ignores the duplicate
And no second terminal fact is appended

### Requirement: Complete semantic replay preserves model-visible provenance

Slice 5 must extend the Slice-1 authority stream with sufficient ordered, versioned evidence to explain admitted operator input; every model-visible context item and tool schema by immutable reference; provider identity, model identity, schema dialect, credential-source class, fallback reason, and route generation; assistant content and committed messages; canonical tool calls/results and their invocation linkage; request and step boundaries; and cancellation, revocation, interruption, abandonment, and terminal closure. Narrative journals, frontend transcripts, metadata checkpoints, arbitrary raw provider payloads, and tool-progress observations must not serve as replay authority. Slice 5 inherits and must not redefine Slice-1 identity, sequence, required-event compatibility, `route.lease_recorded` v1, or terminal semantics.

Task 5.1 must emit exactly `step.started`, `model.request_prepared`, `model.request_route_joined`, `assistant.content_appended`, `assistant.message_committed`, `provider.continuity_stored`, `tool.call_recorded`, `tool.result_recorded`, `model.request_closed`, `step.closed`, and `step.abandoned` v1 for authority-backed sessions. One internal loop iteration has exactly one step. Context-overflow/history repair creates a new request and route lease in that step. Assistant chunks are bounded, coalesced, ordered authority appends before broadcast, not per-token synchronized writes. Hidden reasoning or opaque provider continuity may be retained only when required for continuation, as restricted non-default-projection content references.

Production emission requires a complete current session, turn, and authority scope; a partial or contradictory scope must fail closed, while a sessionless host must continue without fabricated semantic facts. On every abnormal authority-backed host exit, after owned provider/tool cleanup and before `turn.closed`, authority must classify unresolved dispatched or acknowledged invocations under the existing policy, close any open request with its latest response-attempt identity and strongest truthful outcome, and append deterministic reason-bound UUIDv5 `step.abandoned`. Runtime loss uses the same ordering during recovery. Advisory events and future destruction are not terminal authority.

Transport attempts under one joined request must use contiguous response-attempt ordinals on assistant chunks, restricted continuity, message commit, and request closure. A failed attempt must be durably identified and terminalized before retry; its chunks remain canonical but cannot enter the committed message for another attempt, and only provider Done may commit.

Schema-set identity must be content-addressed over canonical ordered schema composition and bind the composition generation plus every schema owner generation. Content references must bind digest, media type, length, storage class, and projection class; the session-adjacent blob store must prevent traversal, substitution, cross-session access, and default-projection access to restricted continuity. Denied tool calls and denied terminal results are canonical. Admitted call/result facts must link without contradiction to existing invocation identity, call identity, owner, and generation facts.

#### Scenario: Provider history is derived after restart
Given a persisted semantic session record
When provider history is rebuilt
Then every model-visible contribution has an attributable event, owner, and generation
And no content is introduced solely from a frontend transcript or narrative journal

#### Scenario: Provider history repair stays in one step
Given a loop iteration has a prepared request and joined route lease
When context overflow requires repaired history and another provider dispatch
Then the first request closes with `superseded_for_context_repair`
And another request with the next ordinal and a distinct route lease is prepared in the same step
And no second `step.started` is appended

#### Scenario: Display content is projected
Given provider tokens have been coalesced into a bounded assistant chunk
When the chunk becomes visible to a frontend
Then `assistant.content_appended` is already durable at its ordinal
And no individual provider token is required to have caused an fsync

#### Scenario: Tool admission is denied
Given a provider emits a well-formed tool call that canonical admission denies
When the step records the decision
Then `tool.call_recorded` records the canonical provider call before admission without an invocation identity
And `tool.result_recorded` records the denied disposition and final model-visible result for the same call identity
And no invocation preparation or dispatch fact is fabricated

#### Scenario: Runtime crashes with an open step
Given replay ends with an open request and open step
When minimum Slice-5.1 recovery runs
Then unresolved dispatched invocations are classified under the existing invocation rules first
And deterministic request abandonment is appended before deterministic `step.abandoned`
And the existing deterministic `turn.closed(interrupted)` remains last
And repeating recovery appends none of those facts again

#### Scenario: Host cancels after durable partial assistant text
Given an authority-backed request has one or more durable assistant chunks but no provider Done
When the host finishes owned cancellation cleanup
Then the open request closes with its latest response-attempt ordinal without an assistant message commit
And deterministic `step.abandoned` is durable before the narrowed turn closure
And a later queued turn can start without waiting for `AgentEnd`

#### Scenario: Authority scope is partial
Given a loop scope contains only some of session identity, turn identity, and authority handle
When execution admission validates semantic emission
Then execution fails closed before provider dispatch
And it does not downgrade to a sessionless semantic or route scope

#### Scenario: Event sequence contains a gap or unsupported required version
Given persisted session events contain a sequence gap or an unsupported required event version
When replay validation runs
Then the runtime refuses to publish a fully recovered session snapshot
And diagnostics identify the first invalid sequence or version without silently skipping required state

#### Scenario: Duplicate event is encountered
Given replay encounters an event identity and sequence already applied
When snapshot reconstruction runs
Then the duplicate is rejected or idempotently ignored according to the compatibility contract
And no state transition is applied twice

### Requirement: Full-spine lineages are forward-only and replay is exact

After the first full-spine event is durable in an authority lineage, every later eligible operation must emit the required full-spine facts. The runtime must not downgrade that lineage, permit a concurrent older writer, negotiate around an older reader, or synthesize authority from a legacy transcript. A reader that does not support a required event or referenced content must fail closed. Missing, substituted, or tampered referenced blobs make full recovery unavailable; diagnostic projections may identify unavailable content, but provider history and exact exports must not contain placeholders.

Strict read-only replay must validate an immutable authority prefix and return reducer state plus its exact cursor without appending recovery facts, acquiring writer authority, mutating caches, consulting mutable transcript state, or publishing projections. Prepared invocations that have no durable dispatch remain incomplete unhanded-off evidence: replay must not add a Slice-3 terminal and provider history must exclude them.

#### Scenario: A mixed lineage reaches its full-spine boundary
Given a legacy authority lineage is idle at a closed turn boundary
When its first full-spine fact is appended
Then every later eligible operation uses the full spine
And no old writer may resume or append a reduced event set
And pre-boundary transcript bytes remain non-authoritative

#### Scenario: A referenced blob is unavailable
Given a required event references a missing or digest-mismatched blob
When full recovery or exact provider-history replay runs
Then recovery fails closed at that event
And no transcript bytes or unavailable placeholder replace the referenced bytes
And a diagnostic-only projection may report the unavailable reference without claiming exact content

#### Scenario: Read-only replay reaches an open prepared invocation
Given the selected valid prefix ends after `invocation.prepared` and before `invocation.dispatched`
When read-only replay reduces that prefix
Then the invocation remains prepared and incomplete
And replay appends no unknown or settlement fact
And provider-history projection excludes the unhanded-off invocation

### Requirement: Response attempts, semantic provenance, and compaction are explicit

Retryable failures under one unchanged request must append `model.response_attempt_failed` v1 before the next contiguous response-attempt ordinal begins. A final failure is bounded by `model.request_closed` and cannot also authorize retry. A zero-text assistant commit is legal only when provider Done commits one or more canonical tool calls under the existing `assistant.message_committed` v1 payload; provider Done with neither content nor calls is not a successful response. Existing event v1 payloads, including `route.lease_recorded`, remain immutable.

After the full-spine boundary, generated system, developer, contribution, and compaction context must point to an authority event. `context.source_materialized` v1 owns ordinary generated context; a committed compaction summary owns compacted context. Provenance may transition from owner-only attribution before the boundary to event-backed attribution after it, but may never transition back or claim a legacy transcript as source authority.

Task 5.2 implements this transition in the authority-backed request writer and v5 reducer. Already-durable legacy owner-only facts remain compatibility input, but a new full-spine request fails closed when any prompt, assistant message, tool result, generated instruction, contribution context, or compaction summary cannot resolve its required source event.

Compaction uses `compaction.started`, `compaction.request_prepared`, `compaction.response_attempt_failed`, `compaction.request_closed`, `compaction.summary_committed`, and exactly one of `compaction.applied` or `compaction.abandoned`, all v1. Turn-owned compaction binds its open turn and step and joins an unchanged route lease. Manual idle compaction uses a session-scoped compaction identity and route evidence, holds the supervisor admission gate, and invents no prompt, turn, or step. The applied fact is durable before one atomic session-context/supervisor projection replacement. Recovery resumes at the first missing deterministic terminal or apply fact and never fabricates a summary.

#### Scenario: Idle manual compaction succeeds
Given a full-spine session is idle with no unresolved invocation or compaction
When manual compaction is admitted
Then it records a session-scoped compaction without prompt, turn, or step identity
And no turn-scoped `route.lease_recorded` fact is fabricated
And prompt admission remains blocked until durable apply or abandonment
And the old context remains visible until the durable applied fact permits one atomic replacement

#### Scenario: Runtime loss follows summary commitment
Given a compaction summary and replacement manifest are committed but apply is absent
When writer-owned recovery runs
Then it verifies every referenced blob and appends the deterministic applied fact
And repeating recovery appends nothing
And it does not invoke the provider again

### Requirement: Projection cursors publish after their output

Every semantic projector must use generic projector cursor v1 bound to projector and projection schema versions, session and stream identities, authority sequence and event identity, output revision, and output digest. Projection output must be fully written, synced, atomically published, and parent-synced before the cursor is written, synced, atomically published, and parent-synced. A cursor may never name output that is absent or has another digest. Stale output with an older cursor is rebuildable; a newer cursor with stale output is corruption and cannot be served.

#### Scenario: Projection crashes before cursor publication
Given a projector has durably published new output
But it crashes before publishing the matching cursor
When the projector restarts
Then the older cursor remains the committed projection frontier
And the mismatched new output is not served under that cursor
And the projector verifies or rebuilds and republishes output before advancing the cursor

### Requirement: Task-5.3 semantic projections are exact, bounded, and shadow-only

Task 5.3 must derive four internal version-1 semantic projections using projector IDs `session.provider-history`, `session.transcript`, `session.frontend-snapshot`, and `session.compaction-checkpoint`, each with projection schema version 1. Provider history must contain immutable exact inputs for each joined provider request and must never synthesize the next request's context. The normal transcript must contain committed messages only. The frontend snapshot may additionally contain durable partial or abandoned assistant chunks plus queue, turn, context, and semantic-conversation state; tool progress remains an ephemeral downstream overlay. Committed content followed by abandonment remains visible with abnormal status.

Every projection must use the frozen availability envelope and deterministic canonical serialization. Full lineages may be exact for the full session. Mixed lineages may publish exact content only for the suffix beginning at the first full-spine boundary and must explicitly report full-session export unavailable. Legacy lineages publish availability envelopes with no exact content claim. Restricted continuity bytes are excluded from all four outputs. Provider history and transcript use immutable bounded chunks and a bounded manifest under the existing 16 MiB cursor-output limit; frontend and compaction checkpoint are bounded single outputs.

One coordinator must coalesce projector wakeups while replaying the latest stable frontier. Each projector publishes independently through generic cursor v1 and retains its prior stable output on failure. Task 5.3 publication is shadow-only and must not switch `ConversationState`, provider dispatch, transcript commands, TUI, ACP, Web, IPC, whole-file snapshots, or compaction compatibility consumers; those migrations remain task 5.4.

The implemented coordinator is one session-scoped worker owned by the authority-backed supervisor. Its capacity-one wake signal and dirty/immediate flags make append notifications loss- and duplicate-safe without making them authoritative. It receives hints only after durable append, coalesces ordinary bursts for 50 ms with a 250 ms maximum delay, and immediately publishes after startup/recovery and step, turn, compaction, explicit-flush, or shutdown boundaries. Shutdown joins owned workers. Atomic replacement clears and stops the old notifier, fences its session-specific root, and transfers its join handle to the replacement supervisor for owned reaping without delaying host publication. Sessionless supervisors must not create a worker. Replay, coordinator, and projector failures are typed and observable, but projection work must not block authority append, loop progress, terminal commitment, replacement, or operator submission.

#### Scenario: Mixed lineage publishes an exact suffix
Given a mixed lineage has a verified full-spine boundary and later complete requests
When the four task-5.3 projectors publish
Then their envelopes identify the first full-spine boundary and claim `exact_suffix` only
And full-session export is explicitly unavailable
And no pre-boundary transcript bytes are included or synthesized

#### Scenario: Committed content is followed by abandonment
Given an assistant message commit is durable before its owning step is abandoned
When transcript and frontend outputs are derived
Then the committed message remains present in both outputs
And it has `abandoned_after_commit` status
And only the frontend may additionally show durable uncommitted chunks as `abandoned`

#### Scenario: A projector fails while peers can advance
Given all four projectors have a committed cursor at an older frontier
When one projector cannot build exact bounded output at the latest stable frontier
Then that projector retains its older output and cursor and reports failure without partial publication
And the other projectors may publish the same latest stable frontier independently
And no compatibility consumer reads any task-5.3 output before task 5.4

### Requirement: Task-5.4 consumers preserve plural authority and exact frontiers

Task 5.4 must implement the task-5.4.0 authority-role matrix without promoting a
projection or compatibility artifact into authority. The append-only semantic
stream owns session, prompt, turn, invocation, model-context, assistant, tool,
compaction, and terminal facts. Provider dispatch must synchronously reduce an
immutable current-context view at the captured latest durable frontier and must
never read lagging provider-history output or depend on the background projector.
`IntentDocument` and plans must use a separate versioned host-state checkpoint;
operator observations must use a separate durable append-only observation
ledger. Friendly name and description are operator-owned metadata. Semantic
counters are derived. Audit and narrative journal remain separate records.

The task-5.4 implementation applies this matrix to provider and compaction
context, host state and observations, catalog, transcript/export, TUI, ACP, Web,
IPC, telemetry, audit, and journal consumers. Task 5.5 retains adverse-consumer
campaigns, and task 5.6 retains public documentation and dual-write closeout.

Validated projection readers must verify schema/projector identity, session and
stream identity, output digest, cursor/output revision, and a real replay
frontier before serving. A UI may show an older valid projection only while
disclosing its source cursor and lag. Provider dispatch and exact resume require
the exact captured frontier; they synchronously reduce or fail closed rather
than use a stale output. Bounds, missing content, unsupported required facts,
unattributed post-boundary context, and frontier mismatch fail before provider
dispatch.

Full lineages may resume and export exactly. Mixed lineages may resume only as an
explicitly labeled legacy compatibility base plus exact semantic suffix; the
legacy base is not historical semantic authority; provider-visible use requires
a newly materialized labeled compatibility context source. Exact full-session
export remains unavailable, and Web historical output contains only the exact suffix. Legacy lineages retain
labeled compatibility resume with no exactness claim. Sessionless semantic
lineage remains deferred.

`/transcript` must denote the exact committed semantic transcript. The current
presentation/evidence export must move to `/session-export` and must disclose
lineage, source frontier, exactness, and stale/partial evidence. Compatibility
`.json` and `.meta.json` dual-write continues through Slice 5.6 closeout. A
rollback may select those mirrors for compatibility consumers, but it must not
stop semantic emission, permit an old writer, downgrade a full-spine lineage, or
make an exact claim from a mirror.

#### Scenario: Projection lags immediately before dispatch
Given a valid frontend or provider-history projection trails the latest durable authority frontier
When an authority-backed request is prepared
Then dispatch captures and synchronously reduces the latest durable frontier
And it does not wait for or read the lagging projector
And reduction failure prevents provider dispatch

#### Scenario: Mixed session resumes
Given a valid legacy base and a verified full-spine suffix
When the operator resumes the session
Then the host labels the base as legacy compatibility content
And appends the exact semantic suffix without merging their provenance
And exact full-session transcript export remains unavailable
And a Web historical reader receives only the exact suffix

#### Scenario: Operator plan state is checkpointed
Given semantic conversation facts and a changed `IntentDocument` or plan
When host state is durably checkpointed
Then the checkpoint binds its own revision to an exact semantic source frontier
And semantic counters are recomputed rather than accepted from the checkpoint
And loss of the checkpoint does not rewrite semantic authority

#### Scenario: Semantic audit input is delivered twice
Given an audit record already names a semantic stream, sequence, event identity, and source kind
When the same source event is delivered again after restart
Then the audit consumer suppresses it by its source dedup key
And it does not suppress a distinct non-semantic audit observation

### Requirement: Task-5.5 adverse-consumer recovery obeys the frozen campaign

Task 5.5 must execute the private semantic protocol's 54 stable scenarios as a
pairwise covering array across lineage, lifecycle, and consumer class. Its fault
and disposition spellings are closed. Exact consumers fail closed when authority
or required owned stores are invalid; projection and frontend consumers may
degrade only with explicit source frontier, lag, lineage, and unavailable state;
audit, journal, telemetry, and compatibility publication remain best-effort only
where their failure cannot be mistaken for semantic success. No campaign case
may promote a projection, host record, evidence ledger, or mirror into authority.

Proven corrupt projector-owned chunks may be quarantined and deterministically
replaced from validated authority only under the owning projector lock. Session
replacement must validate authority, blobs, host-state, observation, and catalog
stores, but may publish when derived projections are missing or damaged if their
unavailability is disclosed and generation-fenced workers rebuild them. A
missing observation ledger may degrade open only when no durable evidence says
one existed; malformed or torn ledger content fails closed. Authority existence
with a missing catalog record is fatal.

ACP worker/supervisor completion is canonical even when terminal notifications
are skipped; notification drain is bounded, authoritative idle reconciliation
releases local busy state, and the next submission remains admissible. IPC lag
must automatically enqueue reconciled current state before subsequent deltas.
Malformed semantic audit rows stop semantic audit cursor advancement and emit a
warning without silently manufacturing duplicate or quarantine records. A
journal that cannot read existing authority reports `semantic_source_unavailable`
rather than sessionless. If semantic save succeeds and compatibility-mirror
publication fails, the caller receives explicit `partial_publication` and the
semantic lineage remains resumable.

The campaign must use copied fixture sandboxes and deterministic injection at
I/O, notification, worker, and replacement boundaries, not timing sleeps. Its
required matrix covers Linux, macOS, and Windows within 15 seconds per platform.
Task 5.5 may fix behavior exposed by frozen red fixtures but may not change event
v1, reducer/cache v5, cursor v1, projection v1, or task-5.4 store schemas. Task
5.6 exclusively owns dual-write removal and developer/applicable public docs.

Every manifest row maps to a concrete exhaustive executor that checks all frozen
fields and invokes its target consumer after the declared fault setup. AC13
removes a real immutable chunk derived from a chunk-bearing mixed-lineage
authority/projection fixture and verifies exact suffix recovery. The macOS
campaign is locally evidenced within budget. GitHub Actions run `32622078435` at
`b788f3b8` passed all applicable Ubuntu and Windows campaign tests within the
15-second platform budget, satisfying the requirement.

#### Scenario: Replacement encounters corrupt derived chunks
Given target authority and required host stores validate but a projector-owned chunk is proven corrupt
When idle session replacement validates the target
Then replacement may publish with that projection explicitly unavailable
And the new-generation worker quarantines and deterministically rebuilds the chunk under its projector lock
And no corrupt bytes or compatibility mirror become authority

#### Scenario: Observation ledger is absent
Given a valid authority-backed session has no observation-ledger file
When resume examines every durable existence marker and finds no evidence that a ledger ever existed
Then resume discloses an empty degraded observation view and continues
But malformed bytes, a torn record, or evidence of prior existence fail closed

#### Scenario: ACP misses terminal notification
Given supervisor completion is canonical and the ACP worker skips its advisory terminal notification
When bounded notification drain ends and authoritative state reports idle
Then ACP clears its local active gate
And a second turn can be submitted without waiting for `AgentEnd`

#### Scenario: Semantic save outlives mirror failure
Given a semantic save is durable and exactly resumable
When the compatibility mirror publication fails
Then the operation returns `partial_publication`
And semantic resumability and forward-only lineage are preserved
And no success result claims that every publication completed

### Requirement: Frontend and host surfaces are authority-narrowing adapters

Every frontend and host adapter must derive actions and projections from the authoritative semantic snapshot and action registry for the session's captured generation, narrowed by transport support and current admission. Projection or transport availability must not grant execution authority, and every execution must re-evaluate current admission through the canonical generation-bound invocation path. No adapter, command alias, generic slash tunnel, RPC method, scheduler, or daemon route may bypass canonical action resolution, admission, or lease issuance.

#### Scenario: Transport does not support an admitted action
Given an action is callable in the runtime generation
And ACP lacks a safe binding for that action
When ACP capability metadata is projected
Then the action is omitted or explicitly unavailable
And generic command tunneling cannot bypass the missing safe binding
