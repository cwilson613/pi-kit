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

Slice 5 must extend the Slice-1 authority stream with sufficient ordered, versioned evidence to explain admitted operator input; every model-visible context item and tool schema by immutable reference or canonical snapshot; provider identity, model identity, schema dialect, credential-source class, fallback reason, and route generation; assistant stream and committed message; invocation identity, owner generation, progress, and terminal result; turn and step boundaries; and cancellation, revocation, interruption, and terminal closure. Narrative journals, frontend transcripts, and metadata checkpoints must not serve as replay authority. Slice 5 inherits and must not redefine Slice-1 identity, sequence, required-event compatibility, or terminal semantics.

#### Scenario: Provider history is derived after restart
Given a persisted semantic session record
When provider history is rebuilt
Then every model-visible contribution has an attributable event, owner, and generation
And no content is introduced solely from a frontend transcript or narrative journal

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

### Requirement: Frontend and host surfaces are authority-narrowing adapters

Every frontend and host adapter must derive actions and projections from the authoritative semantic snapshot and action registry for the session's captured generation, narrowed by transport support and current admission. Projection or transport availability must not grant execution authority, and every execution must re-evaluate current admission through the canonical generation-bound invocation path. No adapter, command alias, generic slash tunnel, RPC method, scheduler, or daemon route may bypass canonical action resolution, admission, or lease issuance.

#### Scenario: Transport does not support an admitted action
Given an action is callable in the runtime generation
And ACP lacks a safe binding for that action
When ACP capability metadata is projected
Then the action is omitted or explicitly unavailable
And generic command tunneling cannot bypass the missing safe binding
