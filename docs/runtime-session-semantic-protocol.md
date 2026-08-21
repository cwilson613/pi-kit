+++
id = "5907b852-acde-4b53-a6b1-2d1ac964868a"
kind = "document"
title = "Runtime session semantic protocol"
status = "decided"
tags = ["runtime", "session", "supervisor", "persistence", "recovery"]
aliases = ["session-semantic-protocol"]
imported_reference = false

[publication]
enabled = false
visibility = "private"

[data]
dependencies = ["selective-kernel-decomposition", "interactive-runtime-supervisor"]
open_questions = []
+++

# Runtime session semantic protocol

## Decision

Slice 1 establishes one append-only authority stream per session for prompt
admission, FIFO queue state, active-turn identity, interruption requests,
minimum invocation identity, and exactly-once turn closure. The stream is the
authority. Persisted snapshots are replaceable reducer caches; `AgentEvent`,
`BusEvent`, frontend state, checkpoints, journals, audit logs, and current
whole-file conversation snapshots are projections or separate records.

This minimum does not claim complete conversation or provider-history replay.
Slice 5 extends the same ordering law with model-context, route, assistant,
tool, step, and compaction facts. Slice 3 adds crash-consistent invocation
lease states; it does not redefine Slice 1 identity or terminal semantics.

## Implementation status

The v1 envelope, all ten fact payloads, strict reducer, identity indexes,
adjacent append/snapshot store, deterministic writer-locked recovery, cache-tail
replay, corruption handling, and Slice-zero maintenance sidecar compatibility
are compiled in `core/crates/omegon/src/session_authority.rs`. Slice 1.3 replaced
the duplicate coordinator/scaffold implementations with one compiled,
frontend-neutral in-memory supervisor while retaining stale-interrupt and
exactly-once settlement protections. Slice 1.4 connected interactive, ACP,
daemon, Web/IPC, and bounded prompt, queue, interruption, and terminal ingress
to that supervisor. Accepted transitions are now synced to the adjacent
authority stream before the owning host mutates or projects runtime state;
whole-file conversation snapshots remain compatibility projections. Transport
acceptance is narrower: IPC and Web may acknowledge runtime ingress before the
supervisor commits the corresponding fact, and daemon or ACP work may wait in
host scheduling before durable prompt admission. Clients reconcile from later
authority-backed queue and lifecycle projections.

An opened authority retains a nonblocking writer lease for its lifetime. A
second process cannot recover or append to that stream while the owner is live,
and session/workspace identity is validated before recovery can append an
interrupted closure. Recovered content-addressed attachments are revalidated by
storage location, type, length, and digest before projection. ACP durably
withdraws queued requests whose response channels were lost with the prior ACP
worker rather than executing them under a later request's waiter.

Slice 1.5 adds a release-coupled compatibility adapter for loop terminal
intents. Authority-backed loop callers submit the captured runtime-turn
identity, explicit outcome, and reason code to the supervisor after owned
cleanup. The supervisor rejects stale intents, treats repeated settlement as
idempotent, and lets an admitted cancellation override a late successful loop
return. `TurnEnd` and `AgentEnd` remain advisory projections and never drive
durable closure. Step, message, continuation, and invocation intent reduction
remains assigned to Slice 4 and Slice 5.

## Identities

- `session_id` is the existing canonical opaque Omegon session ID. It is not
  required to be a UUID.
- `stream_id`, `command_id`, `submission_id`, `prompt_id`, `turn_id`,
  `interruption_id`, and `invocation_id` are lowercase UUIDs.
- `event_id` is a lowercase UUID. Recovery-generated event IDs are UUIDv5
  values derived from the fixed recovery namespace plus stream ID, turn or
  invocation ID, event kind, and recovery-rule version.
- `runtime_generation_id` and owner generation IDs are immutable opaque IDs
  captured in event payloads. They are never inferred from the current process.

Process-local counters, `Instant`, transport request numbers, and frontend
revisions are not durable identities.

## Envelope v1

Every line is one UTF-8 JSON object with these required fields:

```text
envelope_version: 1
event_id: UUID
session_id: string
stream_id: UUID
sequence: u64
event_type: lowercase dotted string
event_version: u16
command_id: UUID
command_fingerprint: lowercase sha256 hex
causation_event_id: UUID | null
recorded_at: RFC3339 UTC timestamp
payload: event-specific object
```

`recorded_at` is diagnostic only. Reducers do not read wall time, current
configuration, current runtime generation, or frontend state.

The authoritative stream contains only state-required events. An unknown event
type, event version, or envelope version stops authoritative replay. Optional
presentation observations belong in derived streams and cannot be made
skippable by an unrecognized envelope.

## Minimum event vocabulary

### `session.created` v1

Sequence 1 only. Payload:

```text
workspace_identity
created_by { principal, ingress }
runtime_generation_id
```

It establishes session and stream identity. No later event may change them.

### `prompt.admitted` v1

Payload:

```text
submission_id
prompt_id
principal
ingress
queue_mode: interrupt_after_turn | until_ready | immediate
content { text, attachments[] }
metadata
```

Admission and FIFO insertion are one transition so no admitted-but-unowned
intermediate state is durable. `queue_mode` records scheduling intent but does
not reorder an already queued prompt. `immediate` starts immediately only when
the session is idle. Attachment entries are immutable content-addressed
references containing digest, media type, byte length, and storage reference;
mutable source paths alone are not replayable authority.

### `prompt.rejected` v1

Payload:

```text
submission_id
principal
ingress
reason_code
```

This records a well-framed, authenticated submission rejected by session state.
Malformed or unauthorized transport input rejected before session command
admission is not written into the session authority stream.

### `prompt.removed` v1

Payload:

```text
prompt_id
reason: withdrawn | session_closing
```

It removes one queued prompt without changing the relative order of survivors.
Slice 1 has no arbitrary reorder or priority event.

### `turn.started` v1

Payload:

```text
turn_id
prompt_id
runtime_generation_id
```

It requires no active turn and the prompt at the selected FIFO queue head. The
transition atomically removes the prompt from the queue and makes the turn
active.

### `turn.interruption_requested` v1

Payload:

```text
interruption_id
turn_id
kind: cancel | revoke
principal
ingress
reason_code
```

Only the first accepted interruption is authoritative. Duplicates are
idempotent; stale or conflicting requests are rejected. The event does not
clear busy state or imply terminal cleanup.

### `invocation.registered` v1

Payload:

```text
invocation_id
turn_id
call_id
owner_generation_id | null
```

This establishes stable invocation identity before execution can become
externally observable. Slice 1 conservatively treats every registered but
unsettled invocation as potentially dispatched after runtime loss. It remains
the compatibility shape for older streams and is not reinterpreted as one of
the newer dispatch phases.

### `invocation.prepared` v1

Payload:

```text
invocation_id
lease_id
turn_id
call_id
deduplication_id | null
invocation_kind
invocation_name
capability_id
contribution_id
owner_generation_id
issue_generation_id
principal
principal_class
surface
admitted_effects
execution
transition
surfaces
```

Preparation is durable after admission and operator approval but before an
execution lease is returned. The optional deduplication identity is present
only when the captured declaration promises owner enforcement for the stable
call ID. Duplicate invocation or same-turn call identities, stale turns, and
post-revocation preparation fail closed.

### `invocation.dispatched` v1

Payload:

```text
invocation_id
lease_id
```

Dispatch is durable after exactly-once lease claim and current-generation
revalidation but before host transport or local owner entry. It must reference
the matching prepared lease and can occur only once. The authoritative JSONL
append remains committed even if replacement of the derived snapshot cache
fails.

### `invocation.acknowledged` v1

Payload:

```text
invocation_id
lease_id
```

Acknowledgement is durable when the selected owner accepts the dispatched
call. Local owners acknowledge on owner entry; host, extension, and MCP
adapters acknowledge at their transport handoff boundary. The invocation and
lease must match the preceding dispatch. Repeated acknowledgement through a
cloned execution control is idempotent and does not append a second fact.

### `invocation.classified_unknown` v1

Payload:

```text
invocation_id
reason_code
recovery_rule_version
```

Recovery appends this for a legacy registered invocation or a new dispatched
or acknowledged invocation that lacks terminal settlement. A prepared call is
retained as prepared because no owner handoff occurred. Live external-owner
transport loss after acknowledgement uses the same durable unknown state and
is not automatically replayed. Unknown classification does not claim success,
failure, or absence of side effects.

Before preparing a call, authority-backed admission checks the stable call ID
against unknown invocations across the session, including prior closed turns.
For a mutating durable unknown, replay is unsafe unless the original persisted
contract was idempotent or carried owner-enforced deduplication with that exact
stable call ID. A current replacement declaration cannot retroactively make the
original handoff safe. Legacy unknown records lack sufficient execution evidence
and fail closed. Unsafe replay is denied as
`invocation:unsafe_unknown_retry` before another lease or `Prepared` fact exists.

This classification does not enable safe replay. Same-turn duplicate call IDs
remain invalid, and no retry-attempt lineage or request fingerprint exists yet.
Provider-request retries occur before completed tool-call dispatch and are not
invocation replay.

### `invocation.settled` v1

Payload:

```text
invocation_id
outcome: completed | failed | cancelled | timed_out | revoked
terminal_evidence_reference | null
```

Settlement is persisted before ordinary completion publication and before the
execution lease closes. Outcomes distinguish completed, failed, cancelled,
timed-out, and revoked execution. Settlement is exactly once. A prior unknown
classification may be reconciled only from authoritative owner evidence under
the Slice 3 policy; reconciliation cannot change an already closed turn
outcome. Settlement durability failure withholds ordinary completion and, for a
mutating declaration, durably fences its declared domain and key.

## Emergency mutation fences

Every mutating execution policy carries a validated mutation domain and fence
key. If acknowledgement, unknown classification, or terminal settlement cannot
be committed after dispatch, the runtime writes an independent version-1
`invocation_mutation_fence` record before returning the durability failure. The
record retains fence, invocation, visible call, capability, owner contribution,
owner generation, composition generation, lease, session, turn, failure phase,
timestamp, and bounded failure-reason identity.

Emergency records live in the shared `invocation-mutation-fences/` directory
beside session authority files, not in the authority JSONL whose failure they
must survive. Each record is append-only, synced before use, strictly decoded,
and deterministically identified. A matching fence denies later authority-backed
mutation before `Prepared`; an unreadable, malformed, or unwritable fence store
fails closed. If the emergency write itself fails, the current runtime poisons
mutation admission in memory. Ordinary execution and restart recovery never
remove a fence. Removal requires deterministic reconciliation or an explicit
audited operator recovery path; no such operator clearing command is exposed by
this slice.

### `turn.closed` v1

Payload:

```text
turn_id
outcome: completed | failed | cancelled | timed_out | revoked | interrupted | unknown
reason_code
recovery_rule_version | null
```

Closure is exactly once and clears the active turn. `completed` means ordinary
successful completion, not merely terminal. `cancelled` requires bounded cleanup
inside Omegon's ownership boundary. `interrupted` is recovery-generated after
runtime loss. `unknown` is used when no stronger safe classification exists.

Slice 1 does not define `session.closed`; session lifecycle closure remains a
separate future requirement.

## Ordering and idempotency

1. One writer lease owns a session stream at a time.
2. Sequence starts at 1 and is contiguous, with no zero, gap, reuse, or wrap.
3. Append admission compares with the durable frontier, not an in-memory count.
4. One line occupies one sequence. Duplicate sequences are corruption even when
   bytes match.
5. The same `(stream_id, sequence, event_id)` redelivered to a projection may be
   ignored. Reusing an event ID at another sequence or with different bytes is
   corruption.
6. The same `command_id` and fingerprint returns its committed result without a
   second append. Reusing a command ID with another fingerprint is refused.
7. A fact is appended, flushed, and synced before the authoritative in-memory
   snapshot advances, any advisory event is published, or command acceptance is
   reported.
8. Append failure leaves the prior snapshot authoritative and publishes no
   corresponding transition.

## Serialization and storage

The initial physical record is an adjacent sidecar:

```text
<session-id>.authority.jsonl
<session-id>.authority.snapshot.json
invocation-mutation-fences/<fence-id>.json
```

Records use strict JSON decoding: duplicate or unknown fields, missing required
fields, invalid enums, oversized records, and trailing non-whitespace fail. One
record is limited to 1 MiB. Blank lines are invalid. Creation syncs the parent
directory; append holds the writer lock, writes the complete newline-terminated
record, flushes, and calls `sync_all` before publication.

A malformed or truncated final line is corruption, not permission to truncate
or repair the authority stream. Existing `<session-id>.json` conversation and
`.meta.json` files remain compatibility projections and are not rewritten into
fictional historical facts.

## Snapshot v2

```text
snapshot_version: 2
reducer_version: 2
session_id
stream_id
last_sequence
last_event_id
state:
  workspace_identity
  runtime_generation_id
  submission, prompt, and turn identity indexes
  queued_prompts[]
  active_turn | null
  accepted interruption history
  invocations[]
  closed_turns[]
  command_receipts[]
```

The reducer is deterministic and side-effect free. A cache is usable only when
its versions are supported, session and stream IDs match, its cursor names the
same event in the authority stream, and all state invariants validate. Otherwise
the runtime rebuilds from sequence 1. A valid cache replays from
`last_sequence + 1`.

The event stream is authoritative. A gap, conflict, unsupported event, invalid
transition, or corrupt record prevents publication of a fully recovered
snapshot and identifies the first invalid sequence.

Reducer invariants:

- at most one active turn;
- a prompt appears at most once across queued and active state;
- a turn starts only from the FIFO queue head;
- no ordinary turn transition follows `turn.closed`;
- a second turn starts only after the previous closure is durable;
- queued prompts survive interruption unless explicitly removed;
- invocation unknown-to-settled reconciliation cannot alter turn closure.

## Recovery

Recovery holds the writer lease. For an open turn it:

1. appends one deterministic `invocation.classified_unknown` for each registered
   unsettled invocation;
2. appends one deterministic `turn.closed(interrupted)`;
3. reconstructs and publishes the recovered snapshot.

Repeated recovery recognizes the deterministic identities and appends nothing.
Late loop, provider, EOF, or process-exit observations cannot append another
closure. Once the interrupted closure is durable, the next queued prompt may
start without waiting for a missed broadcast.

## Compatibility

- Readers accept only envelope version 1 in Slice 1.
- Each `(event_type, event_version)` has immutable field and reducer semantics.
- Required-field, enum, validation, or reducer changes require a new event
  version; meaning is never changed retrospectively.
- Unknown authority events or versions fail replay. They are never silently
  skipped or defaulted.
- Writers emit only versions supported by the configured minimum reader level.
- Snapshot caches may be discarded and rebuilt; authority events may not be
  renumbered, rewritten, or synthesized from legacy transcripts.
- Legacy sessions may continue through the existing compatibility resume path,
  but their historical conversation snapshots are not represented as if they
  had always emitted semantic facts. Starting durable authority creates an
  explicit new stream lineage at the migration boundary.

## Slice ownership

Slice 1 owns supervisor state, ordering, baseline compatibility, deterministic
runtime-loss closure, and conservative unknown invocation classification.

Slice 3 owns policy combination, leases, `Prepared`/`Dispatched`/`Acknowledged`
invocation durability, deduplication, unknown-completion fencing, and safe late
settlement.

Slice 5 extends the authority stream with complete model-context provenance,
route/schema generations, assistant messages/streams, tool calls/results, step
boundaries, compaction, provider-history derivation, and projection-specific
evolution. It inherits and does not redefine Slice 1 identities, sequencing,
required-event compatibility, or turn closure.
