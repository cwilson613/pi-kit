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
Slice 4 adds the minimum pre-dispatch `route.lease_recorded` fact. Slice 5
extends the same ordering law with complete model-context, assistant, tool,
step, compaction, and route-projection facts. Slice 3 adds crash-consistent
invocation lease states; it does not redefine Slice 1 identity or terminal
semantics.

Task 5.0 approved the Slice-5.1 wire contract below. Task 5.1 now emits
`step.started`, `model.request_prepared`, `model.request_route_joined`,
assistant/continuity facts, `tool.call_recorded`, `tool.result_recorded`,
`model.request_closed`, `step.closed`, and `step.abandoned` in production for
complete authority-backed session scopes. The version-4 reducer/cache and
minimum abnormal/recovery terminalization are active. Task 5.2 completes the
frozen compatibility, replay, response-attempt, provenance, compaction, cursor,
recovery, fixture, and atomic session-replacement contract below. Task 5.3
derives the four frozen projections, and task 5.4 now consumes them through the
frozen validated-reader and plural-authority contracts. Task 5.5 completes the
frozen 54-scenario adverse-consumer campaign below with macOS, Ubuntu, and
Windows evidence within budget. Applicable public documentation and dual-write
closeout remain task 5.6.

## Implementation status

The v1 envelope and fact payloads, strict reducer, identity indexes,
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
durable closure. Step, message, continuation, and complete tool-call/result
linkage remain assigned to task 5.1.

Slice 4.5 adds the durable `session.execution_binding_migrated` fact and one
session-lifetime in-memory owner for an atomic loop-driver plus
provider-route-service pair. Durable turn start captures the pair under the same
coordination gate used by migration. A mid-turn replacement request is retained
as in-memory `Pending`; turn closure and the next turn start do not apply it. A
deliberate caller must explicitly invoke `commit_pending_at_quiescence`, whose
authority command is available only while idle and while every invocation is
terminal.

Slice 4.2 adds a versioned route lease before every provider stream. A
session-backed request appends `route.lease_recorded` for the active turn and
reduces it into the authority snapshot. Sessionless work instead appends a
step-wrapped lease to `runtime/route-leases.jsonl` under the Omegon home; that
file is durable route evidence, not a session authority stream or the complete
Slice 5 step protocol. See
[Provider contributions and route leases](provider-contributions-and-route-leases.md).

## Identities

- `session_id` is the existing canonical opaque Omegon session ID. It is not
  required to be a UUID.
- `stream_id`, `command_id`, `submission_id`, `prompt_id`, `turn_id`,
  `interruption_id`, `invocation_id`, `lease_id`, `step_id`, `request_id`,
  `message_id`, `continuity_id`, `tool_call_id`, and `tool_result_id` are
  lowercase UUIDs.
- `event_id` is a lowercase UUID. Recovery-generated event IDs are UUIDv5
  values derived from the fixed recovery namespace plus stream ID, turn or
  invocation ID, event kind, and recovery-rule version.
- `runtime_generation_id` and owner generation IDs are immutable opaque IDs
  captured in event payloads. They are never inferred from the current process.
- Execution-binding driver and provider-route-service generations are validated
  `RuntimeContributionGenerationId` values. Their pair is atomic and is not the
  legacy `runtime_generation_id`, a composition generation, or a per-request
  route-lease contribution generation.

Process-local counters, `Instant`, transport request numbers, and frontend
revisions are not durable identities.

Non-recovery UUID identities are non-nil RFC 4122 UUIDv4 or UUIDv7 values
allocated before their first reference and then reused unchanged. Recovery-only
event and command identities are UUIDv5 values under the fixed recovery
namespace. UUID text is lowercase canonical hyphenated form; another UUID
version, nil UUID, or alternate text form is invalid.

Every entity UUID is unique within its typed stream index and identifies one
immutable entity for the stream lifetime. Provider-visible `call_id` remains the
existing opaque stable call identity and is not converted to a UUID; it is
unique within a turn under the existing invocation rule. Every Slice-5.1
ordinal is a `u32`, starts at zero in its stated scope, is contiguous with no
reuse or gap, and cannot advance after that scope is terminal.

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

### `session.execution_binding_migrated` v1

Payload:

```text
from_generation {
  driver_generation_id
  provider_route_service_generation_id
}
target_generation {
  driver_generation_id
  provider_route_service_generation_id
}
```

The fact atomically changes the durable execution binding only when no turn is
active and no registered, prepared, dispatched, acknowledged, legacy-unknown,
or durable-unknown invocation remains unresolved anywhere in the session. Both
members are required validated contribution-generation IDs. The source must
match the current process-local binding at command admission and, when prior
migration history exists, the reducer's durable binding. An unchanged target is
invalid. The same command ID and fingerprint is idempotent; conflicting command
reuse fails closed.

Opening or resuming a session establishes its boot binding only in the live
authority owner. It does not append this fact. Legacy streams therefore replay
with no durable execution binding until an explicit successful migration, and
no migration history is inferred from `session.created.runtime_generation_id`,
the current process, composition identity, or route leases. Mid-turn pending
replacement is in-memory owner state only. It is never inferred, replayed, or
automatically applied because a turn closed or another turn is about to start.

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

### `route.lease_recorded` v1

Payload:

```text
lease_id
request_id
turn_id
selected_provider_id
selected_model_id
serving_provider_id
serving_model_id
schema_dialect
credential_source_class
fallback_reason | null
contribution_generation_id
route_policy
```

It records the minimum route evidence immediately before provider dispatch.
The turn must be active and match `turn_id`; lease identity is immutable and
unique. Selected identity remains distinct from serving identity when fallback
occurs. Contribution generation and declared fallback compatibility are
revalidated before append, and append failure prevents dispatch. The credential
field may contain specific source-class evidence or the serving contribution's
authentication class when more specific evidence is unavailable. This fact is
not a complete request, response, assistant stream, or provider-history record.

## Slice-5.1 event vocabulary

Task 5.1 adds the following required v1 facts to authority-backed session
streams only. One internal iteration of `loop.rs` is exactly one durable step.
Context-overflow or provider-history repair does not start another step: it
closes the affected request and prepares the next request ordinal under the same
`step_id`. Every such request has a new `request_id` and a distinct route lease.

The required new event types are exactly:

```text
step.started
model.request_prepared
model.request_route_joined
assistant.content_appended
assistant.message_committed
provider.continuity_stored
tool.call_recorded
tool.result_recorded
model.request_closed
step.closed
step.abandoned
```

There is no tool-progress authority fact in task 5.1. Provider token callbacks,
tool progress, spinners, partial JSON parsing, retry notices, and transport
diagnostics remain advisory observations.

Every `reason_code` and `denial_reason_code` is a non-empty stable code of at
most 128 ASCII bytes matching `[a-z0-9_.:-]+`; prose belongs in diagnostics, not
authority payloads. Denial codes reuse canonical invocation-admission codes.
`source_identity`, `owner_id`, provider/model IDs, contribution IDs, capability
IDs, and generation IDs retain their owning contract's validated opaque string
syntax and are serialized without normalization or inference.

### Content references and schema identity

Every Slice-5.1 `content_ref` has this closed shape:

```text
digest_algorithm: sha256
digest: 64 lowercase hex characters
media_type: normalized non-secret media type
byte_length: u64
storage_class: session_blob_v1
projection_class: default | restricted_continuity
```

`media_type` is lowercase ASCII `type/subtype` without parameters. Each Slice-5.1
blob is at most 16 MiB; `assistant.content_appended` applies the smaller chunk
limit below. Empty blobs are legal only where the referencing event explicitly
permits empty content.

The storage key is derived solely from `sha256/<digest>`; no payload stores an
absolute path, relative path, URL, provider object, or arbitrary storage key.
Blobs are written to a content-addressed directory adjacent to the owning
session authority files. Publication uses write, file sync, atomic no-clobber
placement, and parent-directory sync before the authority event that references
the blob. Existing identical bytes may be reused only after digest and length
verification. Reads are descriptor-confined to that session's blob root and
verify storage class, digest, byte length, and media type before decode. A
missing, substituted, cross-session, oversized, or mismatched blob makes the
referencing authority state unrecoverable; it is not replaced from a transcript,
provider cache, URL, or mutable source path.

The store maintains synced digest metadata binding byte length, admitted media
types, and one immutable projection class. A digest cannot be referenced under
both `default` and `restricted_continuity` in the same session; a class mismatch
fails before append rather than weakening or retroactively narrowing access.
Metadata publication precedes the referencing authority append; missing or
contradictory metadata is corruption. Garbage collection may remove
only blobs and metadata unreachable from every retained authority event and
snapshot cursor, and is outside task 5.1.

`default` permits normal model-history and transcript projections subject to
their existing authorization. `restricted_continuity` permits access only to
the provider-continuation adapter captured for the same session, request
lineage, serving provider, and purpose. Default snapshots, transcripts, UI/ACP,
diagnostics, exports, audit display, memory ingestion, extensions, and tools must
not expose or dereference it. Encryption-at-rest policy may further protect the
blob but does not replace these access checks. Hidden reasoning and opaque
provider state use this class only when the serving provider requires exact
continuity for a later request. The runtime stores the minimum provider-defined
continuation bytes, never an arbitrary raw response, headers, request payload,
credential material, transport trace, or catch-all JSON object.

`schema_set_id` is `sha256` over RFC 8785 canonical JSON of this complete
identity object:

```text
schema_set_version: 1
composition_generation_id
normalizer_contribution_id
normalizer_generation_id
schemas: [
  {
    ordinal
    capability_id
    contribution_id
    owner_generation_id
    schema_dialect
    schema_content_ref
  }
]
```

`schemas` is in model-visible order and ordinals are contiguous from zero. The
empty set has the digest of the same canonical object with an empty `schemas`
array and the captured composition generation. Two byte-identical tool schemas
from different compositions or owner generations therefore have different set
identities. A schema content reference always has projection class `default`.
Composition, normalizer, and owner generations are captured, never reconstructed
from the current graph. The normalizer contribution and generation fields are
part of the v1 identity object because byte-identical schemas normalized by a
different generation are not the same provider-facing schema snapshot. This
clarifies the task-5.0 wording before any Slice-5.1 event was emitted.

### `step.started` v1

Payload:

```text
step_id
turn_id
step_ordinal
```

`step_id` is unique within the stream. `step_ordinal` is a `u32`, starts at zero
for each turn, and is contiguous. A turn has at most one open step. A step starts
only for the active turn and after the prior step is durably closed. The event is
appended before the iteration performs context assembly or externally visible
provider/tool work.

### `model.request_prepared` v1

Payload:

```text
request_id
step_id
turn_id
request_ordinal
purpose: initial | context_overflow_repair | provider_history_repair
replaces_request_id | null
continuity_refs[]
context_manifest_id
context_items: [
  {
    ordinal
    role: system | developer | user | assistant | tool
    content_ref
    provenance {
      source_kind
      source_event_id | null
      source_identity | null
      owner_id | null
      owner_generation_id | null
    }
  }
]
schema_set_id
schema_set
```

`request_id` is stream-unique. `request_ordinal` is a contiguous `u32` starting
at zero within the step. Request zero has `purpose: initial` and no replacement.
A later request is legal only after the immediately prior request closed as
`superseded_for_context_repair` or `superseded_for_history_repair`; its purpose
and `replaces_request_id` must match that predecessor. At most one request is
open in a step.

`continuity_refs` is an ordered, duplicate-free list of prior `continuity_id`
values. Request zero may reference continuity from the preceding closed step;
repair requests may also reference continuity from an earlier request in the
same step. Every reference must belong to this session and request lineage. Once
the request route joins, every referenced continuity fact must match its serving
provider, serving model, contribution generation, and `required_for` purpose;
otherwise dispatch is denied. An empty list proves that no restricted provider
continuity is supplied.

Context-item ordinals are contiguous from zero and define exact model-visible
order. `context_manifest_id` is the SHA-256 digest of RFC 8785 canonical JSON of
the ordered context entries, including all content-reference and provenance
fields. `source_kind` is exactly `prompt`, `assistant_message`, `tool_result`,
`system_instruction`, `developer_instruction`, `compaction_summary`, or
`contribution_context`. Prompt, assistant-message, and tool-result entries
require the authority `source_event_id` that established their content; their
`source_identity` names the prompt, message, or result identity. System,
developer, compaction-summary, and contribution context require
`source_identity`, `owner_id`, and `owner_generation_id`; their event ID is
optional until tasks 5.2-5.4 establish or migrate that producer's own semantic
fact. Owner fields are both present or both null. No other enum value or null
attribution is accepted. `schema_set` is
the complete identity object above and must hash to `schema_set_id`. No current
context manager, transcript, or provider history may silently contribute bytes
absent from this manifest.

### `model.request_route_joined` v1

Payload:

```text
request_id
step_id
turn_id
lease_id
```

The matching `route.lease_recorded` v1 must already be durable, name the same
`request_id` and active `turn_id`, and be unjoined. The prepared request must be
open and have no prior join. The join is appended before provider dispatch. A
request has exactly one join and a lease joins exactly one request. The existing
route event and payload do not gain step, context, schema-set, or continuity
fields. A transport retry that preserves the same prepared bytes and serving
route uses the joined lease; any context/history or serving-route re-resolution
closes the request and creates a new request and lease.

### `assistant.content_appended` v1

Payload:

```text
message_id
request_id
step_id
content_kind: text | thinking
response_attempt_ordinal
chunk_ordinal
content_ref
```

There is at most one assistant `message_id` per request. Response
attempt ordinals are contiguous `u32` values from zero under the joined request.
Chunk ordinals are contiguous from zero independently for each `(request_id,
response_attempt_ordinal, content_kind)` channel. Failed-attempt chunks remain
canonical attempt evidence but are never members of another attempt's committed
message. Each non-empty UTF-8 `text/plain` chunk is at most 64
KiB and uses projection class `default`. `text` is ordinary assistant display
content. `thinking` is permitted only for ordinary provider-designated
disclosed reasoning that existing transcript authorization may project; hidden
reasoning needed for exact provider continuation is never ordinary thinking
content and may only use the restricted continuity fact below. No image, tool
call, arbitrary provider object, or opaque bytes are legal assistant chunks.
The stream adapter coalesces until 64 KiB,
50 ms after the first pending byte, or a response boundary, whichever comes
first, but must write and sync the blob and append and sync this fact before
broadcasting that chunk. This is one durable
append per bounded/coalesced chunk, not one `fsync` per provider token. A chunk
cannot follow message commit, request closure, interruption admission, or step
terminalization.

### `assistant.message_committed` v1

Payload:

```text
message_id
request_id
step_id
response_attempt_ordinal
completion_evidence: provider_done
content: [
  {
    content_kind: text | thinking
    chunk_refs[]
    content_digest
  }
]
usage: { input_tokens, output_tokens } | null
tool_call_count
```

`content` contains exactly the non-empty channels in `text`, `thinking` order.
Each `chunk_refs` list exactly equals the declared response attempt's channel
ordinal order, and each `content_digest` is 64 lowercase SHA-256 hex characters
over the exact concatenation of those chunk bytes. This exact manifest is the
commit's ordinal/count evidence; omitted, additional, reordered, or substituted
references reject the commit. `provider_done` means the captured provider
adapter delivered its authoritative Done response boundary. Transport EOF is
not this evidence and cannot commit a message. Usage is optional because not
every provider reports it; each count and their checked sum are at most
1,000,000,000,000 tokens. `tool_call_count` is at most 65,535 and must equal the
canonical calls when the tool-call component is enabled. The
fact commits the default-projection assistant message and may occur at most once
per request and at most once per step; `message_id` is stream-unique. Zero
channels are permitted only when `tool_call_count` is non-zero. The commit is
durable before a complete-message broadcast or conversation-snapshot update.
This required completion field is the smallest v1 correction needed before any
Slice-5.1 message fact was emitted: without it replay could not distinguish Done
from EOF, and no separate stream event family is introduced.

### `provider.continuity_stored` v1

Payload:

```text
continuity_id
request_id
step_id
response_attempt_ordinal
serving_provider_id
serving_model_id
provider_contribution_generation_id
continuity_kind: hidden_reasoning | opaque_provider_state
required_for: next_request
restricted_required: {
  allowed_kinds[]
  max_blob_bytes
}
content_ref
```

The content reference must be `restricted_continuity`. The serving identities
and contribution generation must exactly equal the joined route evidence. The
fact is optional, may repeat for distinct continuity kinds, and is legal only
when the captured contribution's generation-bound continuity policy is
`restricted_required`. Its durable policy evidence has a sorted, duplicate-free,
non-empty subset of the two closed kinds and a non-zero size ceiling no larger
than 16 MiB, declares the stored kind, and proves the serving adapter reported
those exact bytes required to continue. Continuity bytes use
`application/octet-stream`; this closed payload has no raw response, headers,
request, credential, transport trace, provider object, or arbitrary JSON field.
Policy `none`, an undeclared kind, or a blob above the declared ceiling rejects
the fact. `(request_id,
continuity_kind)` is unique. It is durable before any later prepared request can
claim the continuity. Reducers retain reference metadata but default projections
omit both metadata and bytes.

### `tool.call_recorded` v1

Payload:

```text
tool_call_id
request_id
step_id
call_ordinal
call_id
invocation_name
arguments_ref
```

`tool_call_id` is stream-unique; provider-visible `call_id` remains the stable
call identity used by invocation facts. `call_ordinal` is contiguous from zero
across the step, including calls from repaired requests. Arguments use a
non-empty `default` content reference. The call ID and invocation name are
non-empty and bounded. The matching assistant message must already be committed,
and its declared `tool_call_count` bounds canonical calls. This fact records the
provider observation before admission; it intentionally contains no admission,
invocation, lease, contribution, owner, or denial fields. Denied and
not-dispatched calls therefore remain canonical without fabricating invocation
authority. If admission succeeds, the later `invocation.prepared` must match the
call ID, invocation name, turn, and active step. The call fact must precede
preparation and dispatch, and its request must close before dispatch, so no owner
handoff can occur before the canonical provider response exists.

### `tool.result_recorded` v1

Payload:

```text
tool_result_id
tool_call_id
step_id
result_ordinal
call_id
disposition: denied | settled | unknown_completion | not_dispatched
invocation_id | null
lease_id | null
content_ref
is_error
reason_code | null
```

`tool_result_id` is stream-unique. `result_ordinal` is contiguous from zero
across terminal results in provider call order, so each result ordinal equals
its call ordinal. Exactly one result may terminate each recorded call. Every
result has one non-empty `default` content reference containing the final
redacted, truncated, and enriched model-visible bytes; raw or pre-transformation
output is not authority. `denied` and `not_dispatched` require `is_error`, a
bounded reason, and null invocation/lease linkage, and they contradict any
existing invocation for the call. `settled` requires the exact invocation and
lease from matching terminal durable invocation facts, has no disposition
reason, and derives `is_error` from whether settlement is `completed`.
`unknown_completion` requires the exact invocation and lease from matching
`invocation.classified_unknown`, repeats its bounded reason, and is an error.
The result fact is durable before model-history insertion, frontend publication,
or a later step's context manifest references it. Task 5.1 defines no
intermediate result or progress event.

### `model.request_closed` v1

Payload:

```text
request_id
step_id
response_attempt_ordinal
outcome: response_completed | provider_failed | eof | cancelled | timed_out |
  revoked | superseded_for_context_repair |
  superseded_for_history_repair | abandoned | unknown
reason_code
recovery_rule_version | null
```

Each prepared request closes exactly once. `response_completed` requires its
assistant message committed with `provider_done`, including a legal zero-text
tool-call message, and the closure attempt ordinal must equal the committed
attempt. Once a message is committed, only `response_completed` may close that
request during ordinary execution; deterministic recovery or host abnormal
terminalization may instead preserve the commit and close the still-open request
as `abandoned`.
Repair outcomes permit exactly the next request described above. EOF is explicit
and cannot be projected as an ordinary complete response unless a provider
adapter had already supplied authoritative completion. `abandoned` is emitted
only by deterministic recovery or host abnormal terminalization when no stronger
live request outcome is safe; `unknown` is used only where no stronger
classification is safe. Closure does not itself close the step or turn. Until
`assistant.message_committed` is reduced by its owning component,
`response_completed` fails closed because its required commit cannot yet be
proved; callers use the strongest other truthful outcome rather than fabricate
assistant state.

Transport retries that preserve one prepared request and joined route assign a
contiguous response attempt ordinal before each bridge call. Before a later
attempt begins, the failed attempt is synchronously terminalized in the
route-service failure record with request identity, attempt ordinal, and truthful
failure class. Failure to persist that evidence stops retry. Such terminalization
never supplies `provider_done`, never commits a message, and does not close the
request while a policy-authorized transport retry remains possible.

### `step.closed` v1

Payload:

```text
step_id
turn_id
outcome: continue_loop | turn_completed | failed | eof | cancelled |
  timed_out | revoked | unknown
reason_code
```

Normal closure requires every request closed, every recorded call to have one
terminal result, and no nonterminal invocation owned by the step. It occurs
exactly once and before a later `step.started` or `turn.closed`. `continue_loop`
means tool results or other canonical state feed the next internal iteration;
`turn_completed` means the assistant iteration proposes ordinary successful turn
closure. EOF, cancellation, timeout, and revocation remain distinct and may only
narrow the later turn outcome under existing terminal rules.

The turn mapping is exact: `turn_completed` permits only
`turn.closed(completed)`; `failed` and `eof` permit only
`turn.closed(failed)`; `cancelled`, `timed_out`, `revoked`, and `unknown` permit
only the same-named turn outcome. `continue_loop` forbids turn closure and
requires the next contiguous step unless an already admitted interruption
narrows terminalization first.

For v1, continuation is encoded by `outcome` rather than a second payload field.
`continue_loop` requires a completed committed message with at least one fully
recorded call/result; `turn_completed` requires a completed committed message
with no calls. Each failure outcome must match the final request outcome. A next
step is legal only after `continue_loop`; turn closure is legal only after the
corresponding terminal outcome mapping.

### `step.abandoned` v1

Payload:

```text
step_id
turn_id
reason_code
recovery_rule_version
```

Recovery and host-owned abnormal terminalization emit this event. After provider
and tool cleanup, authority first classifies unresolved dispatched or
acknowledged invocations, then closes any open request with the strongest
truthful outcome and response-attempt ordinal, and finally abandons the open
step before `turn.closed`. A committed message with no request closure is
preserved and the request is deterministically `abandoned`; normal completion is
not invented. The bounded stable reason identifies runtime loss, cancellation,
timeout, provider/worker failure, or host exit. UUIDv5 event and command
identities bind the recovery namespace, step ID, event kind, reason, and rule
version. Repetition is idempotent, and abandonment is never successful step
completion. It may retain prepared invocations or unknown invocation/results.

Interactive cancellation keeps polling the same pinned loop execution during a
bounded cooperative grace period. Canonical abandonment begins only after that
execution returns, or after the supervisor aborts it and joins the turn-scoped
event relay that was its sole capability to project chunks, messages, tool
progress/results, or terminal events. Aborting drops the turn's authority and
conversation borrows; process-backed tools additionally retain tree-scoped drop
cleanup. Native HTTP parser tasks may continue only as transport work after
their receiver is dropped. Their remote completion is unverified, not reported
as cleaned up, and cannot append authority facts or project into this or a later
turn.

### Slice-5.1 ordering and cardinality

Within an authority-backed step, the required partial order is:

```text
step.started
  -> model.request_prepared
  -> route.lease_recorded
  -> model.request_route_joined
  -> provider dispatch
  -> assistant.content_appended* / provider.continuity_stored*
  -> assistant.message_committed
  -> tool.call_recorded* / invocation.prepared*
  -> model.request_closed
  -> invocation.dispatched / invocation.acknowledged /
     invocation terminal facts and tool.result_recorded* where calls exist
  -> step.closed
  -> turn.closed or the next step.started
```

Provider response forms may interleave content and call discovery, but ordinals
within each scope remain contiguous and every corresponding fact precedes its
projection. For an admitted call, `tool.call_recorded` precedes
`invocation.prepared`; assistant commit precedes the call, and request closure
precedes `invocation.dispatched`. A repair request starts only after the prior request
closure and before invocation/result work for a response that was superseded.
An admitted interruption forbids new steps, requests, chunks, continuity, and calls; only
already-authoritative invocation/result cleanup plus request, step, and turn
terminal facts may follow. No event may attach to a closed request or terminal
step except those explicitly allowed above.

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
<session-id>.authority.blobs/sha256/<digest>
<session-id>.authority.blobs/sha256/<digest>.meta.json
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

Sessionless route leases are separate from these per-session files. They append
versioned step wrappers to `runtime/route-leases.jsonl` under the Omegon home,
with an ephemeral `step_id`, timestamp, and the route lease. They carry no
fabricated session or turn identity. No historical route-lease listing command
is implemented. Task 5.1 does not add assistant, context, schema, continuity,
tool, request, or step facts to that file. A sessionless full semantic stream
requires a later design and remains deferred with Slice-5 consumer work.

## Task-5.2.0 compatibility and replay freeze

This section is normative for tasks 5.2 through 5.5. It adds event names needed
by those tasks but does not change an existing event v1 payload. In particular,
`route.lease_recorded` v1 remains exactly the turn-owned payload above.

### Authority-lineage levels

An authority stream has exactly one derived lineage level:

| Level | Definition | Recovery and writing | Provider history and exact export |
| --- | --- | --- | --- |
| `legacy` | No full-spine event has occurred. This includes sessions with no authority stream and authority streams containing only pre-full-spine facts. | Existing no-stream compatibility resume or the supported baseline authority reducer applies. A first full-spine operation may start only while idle at a closed turn boundary. | Historical conversation bytes are compatibility input only. No semantic provider-history or exact-export claim is available. |
| `mixed` | A baseline authority prefix is followed by the first full-spine fact at a recorded sequence boundary. | The prefix retains its original semantics. From the boundary forward, every eligible operation must use the full spine. Recovery never fills the prefix with invented facts. | Exact request/provider history is available only for independently complete requests in the full-spine suffix. An exact export that requests pre-boundary semantic content is unavailable rather than placeholder-filled. |
| `full` | Every eligible operation since `session.created` used the full spine. | Strict full-spine reduction and recovery apply throughout. | Provider history and exact exports are available only when all required events and blobs verify. |

The first of `step.started`, `context.source_materialized`, or
`compaction.started` establishes the full-spine boundary. Once that fact is
durable, lineage level is forward-only. A later writer must emit every required
fact for every eligible operation; it cannot append a baseline-only turn,
downgrade the stream, or omit an event because an older reader is present. There
is no configured minimum-reader-level negotiation, old-writer compatibility
mode, or concurrent old/new writer support. The existing exclusive writer lease
prevents concurrent appenders. A reader that does not understand any required
event or version fails closed at that sequence.

A no-stream legacy session begins a new authority lineage at its migration
boundary. It is not a `mixed` stream until actual authority facts precede a
full-spine boundary in that same stream. Conversation snapshots, metadata,
journals, audit records, route-only sessionless records, and provider caches are
never converted into historical authority events.

Diagnostic projections may retain event identity and render
`content_unavailable` with no substituted bytes when a blob cannot be read.
Provider history, model requests, compaction input, transcript/exact export, and
any projection labeled exact must instead return unavailable and identify the
first failing sequence/reference. They may not send a placeholder to a provider
or serialize one as if it were original content.

### Response-attempt lineage

`model.response_attempt_failed` v1 is a new required event when one joined
request will retry without changing its prepared bytes or serving route.
Payload:

```text
request_id
step_id
response_attempt_ordinal
failure: provider_error | eof | timed_out | transport_lost
reason_code
retry_disposition: retry_same_request
```

Attempt zero begins with the first bridge call. Each later attempt begins only
after the preceding attempt's failure fact is durable. Attempt ordinals are
contiguous and shared by chunks, continuity, message commit, retry failure, and
request closure. A retry failure fact requires no commit for that attempt and is
the terminal boundary for all of its chunks and continuity. No later fact may
attach to the failed attempt. Failure to append this boundary prevents retry.

A failure that will not retry does not emit `model.response_attempt_failed`;
`model.request_closed` terminalizes its attempt with the strongest truthful
outcome. A request cannot both close an attempt and retry it. Route or prepared-
request changes close the request and create the next request ordinal rather
than creating another attempt.

Provider Done may commit an attempt with zero `text`/`thinking` channels only
when `tool_call_count` is non-zero, exactly as already frozen by
`assistant.message_committed` v1. That is a zero-content tool-call commit, not an
empty response. Provider Done with no channels and no calls cannot commit and
closes with a truthful non-success reason such as `provider_empty_response`.
Failed-attempt content remains durable attempt evidence but is excluded from
provider history and from every later attempt's commit.

### Semantic provenance transition

`context.source_materialized` v1 establishes generated model-visible source
bytes that are not already owned by a prompt, assistant message, tool result, or
compaction summary. Payload:

```text
context_source_id
source_kind: system_instruction | developer_instruction | contribution_context
source_identity
owner_id
owner_generation_id
content_ref
```

The identity is stream-unique and immutable. Its content reference is
non-empty, `default`, and belongs to the session blob store. Reusing the same
source bytes is permitted by referencing the same fact; changing bytes, owner,
generation, kind, or identity requires a new fact. The fact is durable before a
request manifest can reference it.

Before a mixed lineage's boundary, existing `model.request_prepared` v1 permits
owner-attributed generated context with a null `source_event_id`. At and after
the boundary, system, developer, and contribution items require the matching
`context.source_materialized` event ID; compaction items require the matching
`compaction.summary_committed` event ID. Prompt, assistant-message, and tool-
result attribution remains unchanged. Event-backed provenance never transitions
back to owner-only provenance, crosses a session, or names legacy transcript
bytes as authority. This is a reducer rule; no existing v1 payload is widened.

### Compaction vocabulary

Compaction has one `compaction_id`, zero or more contiguous
`compaction_request_id` values, and one terminal. Its owner scope is the closed
tagged shape:

```text
{ kind: turn, turn_id, step_id }
{ kind: session_idle }
```

A turn owner requires the named active turn and open step. `session_idle`
requires no active turn, open step, unresolved invocation, or other compaction.
It is used by manual idle compaction and does not create a prompt, prompt queue
entry, turn, or step. Compaction policy may choose not to compact without
writing a fact; once `compaction.started` is durable, the operation must reach
one frozen terminal below.

#### `compaction.started` v1

Payload:

```text
compaction_id
owner_scope
trigger: manual_idle | context_pressure | context_overflow
source_frontier { sequence, event_id }
source_context_revision
input_manifest_id
input_items: [
  { ordinal, source_event_id, source_identity, content_ref }
]
retained_items: [
  { ordinal, source_event_id, source_identity, content_ref }
]
target_context_revision
```

The source frontier must be the current authority frontier. Input and retained
ordinals are independently contiguous from zero, every source event and content
reference verifies, and the two lists are duplicate-free. `input_manifest_id`
is SHA-256 over RFC 8785 canonical JSON of both ordered lists plus source
frontier/revision and owner scope. The target revision is exactly the next
session context revision and is reserved by this event. Idle start acquires the
supervisor admission gate before append and retains it through terminalization,
so prompt/turn admission cannot race the replacement.

#### `compaction.request_prepared` v1

Payload:

```text
compaction_request_id
compaction_id
request_ordinal
replaces_compaction_request_id | null
prompt_template { owner_id, owner_generation_id, content_ref }
route:
  { kind: turn_lease, lease_id }
  | {
      kind: session_idle
      selected_provider_id
      selected_model_id
      serving_provider_id
      serving_model_id
      schema_dialect
      credential_source_class
      fallback_reason | null
      contribution_generation_id
      route_policy
    }
```

Request ordinals are contiguous from zero. A replacement may follow only a
`compaction.request_closed` failure and names the immediately prior request. The prompt
template is exact provider-visible instruction content; compaction input bytes
come from `compaction.started` and are not duplicated in this fact. The empty
tool-schema set is implicit and compaction requests cannot invoke tools.

Turn-owned compaction requires a previously durable, unjoined
`route.lease_recorded` v1 whose `request_id` equals
`compaction_request_id` and whose turn matches the owner scope. The compaction
request claims that lease; no `model.request_route_joined` is fabricated.
Session-idle compaction records the same route semantics inline because
`route.lease_recorded` v1 requires a turn. The prepared fact is synced before
provider dispatch in both forms. Inline idle evidence is authority for this
compaction only and is not a general route lease.

#### `compaction.response_attempt_failed` v1

Payload:

```text
compaction_request_id
compaction_id
response_attempt_ordinal
failure: provider_error | eof | timed_out | transport_lost
reason_code
retry_disposition: retry_same_request
```

It has the same contiguous, append-before-retry, no-later-output law as
`model.response_attempt_failed`. Changing route, template, input, or serving
identity requires a terminal request failure and the next compaction request,
not another response attempt.

#### `compaction.request_closed` v1

Payload:

```text
compaction_request_id
compaction_id
response_attempt_ordinal
outcome: summary_committed | provider_failed | eof | cancelled | timed_out |
  superseded_for_route_change | abandoned | unknown
reason_code
recovery_rule_version | null
```

Each prepared compaction request closes exactly once. `summary_committed`
requires the matching summary commit at the same attempt ordinal and is the only
successful outcome. A replacement request is legal only after
`superseded_for_route_change`. A final response-attempt failure closes the
request rather than also emitting a retry boundary. Recovery uses `abandoned`
only when no stronger outcome is safe.

#### `compaction.summary_committed` v1

Payload:

```text
compaction_summary_id
compaction_request_id
compaction_id
response_attempt_ordinal
completion_evidence: provider_done
summary_ref
summary_digest
replacement_manifest_id
replacement_items: [
  {
    ordinal
    source_kind: compaction_summary | retained
    source_event_id
    source_identity
    content_ref
  }
]
usage: { input_tokens, output_tokens } | null
```

The summary identity is stream-unique. `summary_ref` is non-empty UTF-8
`text/plain`, `default`, and `summary_digest` verifies its exact bytes. Provider
Done is required; EOF cannot commit. Replacement items are contiguous and
contain exactly one `compaction_summary` item naming this commit event and its
summary identity/ref plus the unchanged retained items from start in their
declared order. `replacement_manifest_id` is SHA-256 over RFC 8785 canonical
JSON of the replacement list, target revision, and compaction identity. A
request and compaction have at most one summary commit.

#### `compaction.applied` v1

Payload:

```text
compaction_id
compaction_summary_id
source_context_revision
target_context_revision
replacement_manifest_id
recovery_rule_version | null
```

Apply requires the exact started and summary facts and re-verifies every
referenced blob. It is the sole authority transition from source to target
context revision. The fact is appended and synced before the session context
projection and supervisor-visible context pointer are replaced together. They
must expose either the complete source revision or the complete target revision,
never a mixed list. For idle compaction, successful atomic replacement releases
the admission gate. Append or replacement failure retains the source projection;
writer-owned recovery completes the deterministic replacement before admitting
another turn.

#### `compaction.abandoned` v1

Payload:

```text
compaction_id
reason_code
last_compaction_request_id | null
last_response_attempt_ordinal | null
recovery_rule_version
```

Abandonment is the only terminal when no summary was committed. It preserves the
source context revision and releases an idle admission gate only after the fact
is durable. A committed summary is never abandoned: recovery applies it. The
last identities are null only when no compaction request was prepared. A
prepared request with no later response evidence conservatively records response
attempt zero because the pre-dispatch fact cannot prove that handoff did not
occur. Abandonment does not create an assistant message, tool result, prompt,
turn, step, or normal compaction summary.

The required order is:

```text
compaction.started
  -> compaction.request_prepared
  -> provider dispatch
  -> compaction.response_attempt_failed* -> retry, or
     compaction.summary_committed
  -> compaction.request_closed
  -> compaction.applied
```

Route/template changes may repeat `compaction.request_prepared` at the next
request ordinal after the prior request closes as
`superseded_for_route_change`. Without a summary, recovery first closes an open
request as `abandoned` and then appends `compaction.abandoned`. For turn-owned
compaction, apply or abandonment precedes request/step/turn terminalization. For
idle compaction, no turn closure is involved.

### Read-only replay API

Task 5.2 exposes a kernel-internal read-only replay operation with semantic
inputs equivalent to:

```text
replay_prefix(session_id, stream_id, end_sequence | end_event_id | end_of_stream)
  -> { lineage_level, reducer_state, authority_cursor }
```

Exactly one end selector is accepted. The operation opens a stable read view,
strictly decodes from sequence one or a fully verified reducer cache, verifies
event transitions and every content reference needed by reduced state, and
returns the state after applying the selected event exactly once. The returned
cursor is `{ session_id, stream_id, sequence, event_id }`. A selected event ID
must occur at its unique sequence. A moving/truncated file, unsupported event,
gap, duplicate, invalid transition, missing blob, or cache/cursor mismatch fails
without a partial state.

Read-only replay does not acquire the writer lease, append recovery or terminal
facts, update caches, read a mutable conversation snapshot, publish advisory
events, invoke providers/tools, or advance a projector cursor. In particular,
`invocation.prepared` without `invocation.dispatched` remains incomplete
unhanded-off evidence. Replay adds no Slice-3 terminal and provider history
excludes the invocation and any uncommitted request output. Writer-owned full
recovery is a separate operation and must be requested explicitly.

### Generic projector cursor v1

Every durable semantic projector uses this strict cursor object:

```text
cursor_version: 1
projector_id
projector_version
projection_schema_version
session_id
stream_id: uuid | null
last_sequence
last_event_id
output_revision
output_digest_algorithm: sha256
output_digest
```

`projector_id` identifies provider history, transcript, frontend snapshot,
compaction checkpoint, or another declared projection. Projector and schema
versions are non-zero integers. `output_revision` starts at one and advances by
one for each committed output. Sequence zero has no cursor file; a nonzero
sequence/event pair must exist in the named stream. The digest covers the exact
published output bytes, not a parsed object.

Publication obeys output-before-cursor:

1. Build output solely from a successful read-only replay prefix.
2. Write and sync a temporary output, atomically replace the output, and sync its
   parent directory.
3. Write and sync the matching temporary cursor, atomically replace the cursor,
   and sync its parent directory.
4. Only then publish or serve the new revision.

A projector never advances the cursor before output publication and never
mutates committed output in place. New output with the old cursor after a crash
is uncommitted and must be verified/rebuilt before cursor advance. A cursor that
names absent output, another digest/revision, an impossible authority position,
or unsupported projector/schema versions is invalid; the consumer discards its
projection state and rebuilds from authority. Duplicate delivery at or below the
cursor is ignored only after event identity matches. A gap above it triggers
replay from authority, never blind delta application.

## Task-5.3.0 concrete projection freeze

This section is normative for task 5.3. It freezes projection DTOs and behavior
without activating a projector or changing a consumer. All DTOs reject unknown
fields. Integer bounds are checked before allocation, UUIDs use lowercase
hyphenated text, and digests use 64 lowercase hexadecimal SHA-256 characters.

### Identities and common availability envelope

The only task-5.3 projector identities are:

| Projector ID | Projector version | Projection schema version | Body form |
| --- | --- | --- | --- |
| `session.provider-history` | 1 | 1 | immutable chunks plus manifest |
| `session.transcript` | 1 | 1 | immutable chunks plus manifest |
| `session.frontend-snapshot` | 1 | 1 | bounded inline snapshot |
| `session.compaction-checkpoint` | 1 | 1 | bounded inline checkpoint |

Every cursor output is exactly one `ProjectionEnvelopeV1`:

```text
envelope_schema_version: 1
projector_id: session.provider-history | session.transcript |
  session.frontend-snapshot | session.compaction-checkpoint
projector_version: 1
projection_schema_version: 1
session_id
stream_id
lineage_level: legacy | mixed | full
availability: unavailable | available
exactness: none | exact_suffix | exact_full
scope: none | full_spine_suffix | full_session
full_spine_boundary: { sequence, event_id } | null
source_frontier: { sequence, event_id } | null
full_session_export: unavailable | available
unavailable: {
  reason: legacy_lineage | pre_boundary_content_not_authoritative
  first_sequence: u64 | null
  content_digest: string | null
} | null
payload: { kind: none } |
  { kind: chunk_manifest, manifest: ChunkManifestV1 } |
  { kind: frontend_snapshot, snapshot: FrontendSnapshotV1 } |
  { kind: compaction_checkpoint, checkpoint: CompactionCheckpointV1 }
```

The enum spellings above are closed and stable for schema v1. `legacy` requires
`unavailable`, `none`, `none`, null boundary, unavailable full-session export,
`unavailable.reason: legacy_lineage`, and `payload.kind: none`; it makes no
content claim. A no-authority legacy session has null `stream_id` and
`source_frontier`; its availability envelope is rebuilt from session identity
and has no cursor file because there is no authority position to name. A legacy
authority stream uses its real stream and frontier and the generic cursor. This
exception never creates a stream or synthetic sequence. `mixed` requires its
first full-spine boundary, `available`,
`exact_suffix`, `full_spine_suffix`, unavailable full-session export, and
`unavailable.reason: pre_boundary_content_not_authoritative`. Its payload
contains only items whose complete authority starts at or after the boundary.
`full` requires a null boundary, `available`, `exact_full`, `full_session`,
available full-session export, and null `unavailable`. Every non-null source
frontier is nonzero and exactly matches the generic cursor frontier.

An invalid stream, unsupported event, missing or mismatched default blob, or
authorization failure does not produce an availability envelope because no
exact source frontier was reduced. The projector reports the typed failure out
of band and retains its last committed output and cursor. The two
`unavailable.reason` values therefore describe valid lineage limitations, not a
way to publish around corruption. A mixed-lineage full-session export request
returns this envelope with no substituted prefix bytes; suffix access follows
the payload already committed by the projector.

### Chunk and manifest DTOs

Provider history and transcript use the same exact manifest and chunk wrapper:

```text
ChunkManifestV1 {
  manifest_schema_version: 1
  projector_id: session.provider-history | session.transcript
  session_id
  stream_id
  source_frontier: { sequence, event_id }
  chunk_count: u32
  item_count: u64
  chunks: [
    {
      chunk_ordinal: u32
      chunk_id
      first_item_ordinal: u64
      last_item_ordinal: u64
      item_count: u32
      byte_length: u64
      digest_algorithm: sha256
      digest
    }
  ]
}

ProjectionChunkV1 {
  chunk_schema_version: 1
  projector_id: session.provider-history | session.transcript
  session_id
  stream_id
  chunk_ordinal: u32
  first_item_ordinal: u64
  last_item_ordinal: u64
  items: [ProviderRequestInputV1] | [TranscriptMessageV1]
}
```

Chunk ordinals and item ordinals are contiguous from zero. An empty projection
has an empty manifest and no chunks. Each chunk has at most 4,096 items and at
most 8 MiB of canonical bytes. The manifest/envelope and each inline output are
independently at most the generic 16 MiB projection-output limit. Projection
items contain bounded content references or bounded prompt content, not copied
blob bytes, so an individual legal item must fit an 8 MiB chunk; otherwise exact
projection fails without advancing the cursor. Splitting starts at item zero and
greedily appends the next whole item while both limits remain satisfied; it then
closes that chunk and continues, so batching cannot alter chunk boundaries.
`chunk_id` and the manifest entry digest are SHA-256 over the exact canonical
chunk bytes. Manifest counts, ranges, byte lengths, IDs, and digests must all
agree before serving.

Chunks are written and synced under the projector lock before the temporary
manifest envelope is published through output-before-cursor. Chunks are
immutable and content-addressed; an existing path with different bytes is
corruption. Each projector exclusively owns its chunk namespace. Published
chunks are retained for the lifetime of the authority lineage, matching the
authority/blob retention boundary. Only temporary or never-manifested chunks
may be removed, and only while holding that projector's lock after proving that
no committed manifest references them. Task 5.3 does not introduce independent
age-, count-, or reader-based chunk garbage collection.

### Provider-history DTO and semantics

Each provider-history item is the immutable input of one joined request:

```text
ProviderRequestInputV1 {
  item_ordinal: u64
  request_id
  step_id
  turn_id
  request_ordinal: u32
  purpose: initial | context_overflow_repair | provider_history_repair
  replaces_request_id: uuid | null
  prepared_event: { sequence, event_id }
  route_join_event: { sequence, event_id }
  lease_event: { sequence, event_id }
  lease_id
  selected_provider_id
  selected_model_id
  serving_provider_id
  serving_model_id
  schema_dialect
  credential_source_class
  fallback_reason: string | null
  contribution_generation_id
  route_policy
  continuity_ids: [uuid]
  context_manifest_id
  context_items: [{ ordinal, role, content_ref, provenance }]
  schema_set_id
  schema_set
}
```

Route fields, context entries, provenance, and schema-set fields are copied
exactly from the joined lease and prepared request; their nested DTOs are the
unchanged authority-v1 shapes. `continuity_ids` proves which restricted facts
were supplied but neither continuity metadata nor bytes are projected. Joined
requests are ordered by prepared-event sequence. A prepared-but-unjoined request
is excluded. A joined request is included immediately and its item never changes
when response evidence or request closure later arrives. Failed-attempt chunks,
response output, request outcomes, and prepared-only invocations are not
provider inputs and are excluded. Every repair request is a distinct item with
its own exact manifest and lease. A projector must never infer, merge, trim, or
synthesize a later request from an earlier request, transcript, current context,
provider cache, or compatibility conversation. Provider dispatch continues to
use the authority-backed request writer, never this projection.

### Transcript DTO and commitment rules

The normal transcript contains only durable committed messages:

```text
TranscriptMessageV1 {
  item_ordinal: u64
  message_kind: prompt | assistant | tool_result
  role: user | assistant | tool
  message_id
  turn_id: uuid | null
  step_id: uuid | null
  request_id: uuid | null
  source_event: { sequence, event_id }
  content: { prompt_content } |
    { assistant_channels: [{ content_kind, chunk_refs, content_digest }] } |
    { tool_result_id, tool_call_id, call_id, content_ref, is_error, disposition }
  status: normal | abandoned_after_commit
}
```

`prompt_content` is the immutable admitted prompt content, including attachment
references. Prompt, assistant, and tool-result items are ordered by source-event
sequence, then by their authority ordinal where one event owns an ordered list.
An assistant item exists only after `assistant.message_committed`; a tool item
exists only after `tool.result_recorded`. Raw assistant chunks, failed attempts,
prepared requests/invocations, route evidence, and progress are not transcript
messages. If a committed item's owning request, step, or turn later reaches an
abnormal abandonment terminal, the item remains visible and its status is
`abandoned_after_commit`; terminalization never retracts committed content.

### Frontend snapshot DTO and incomplete state

The bounded single `FrontendSnapshotV1` is:

```text
FrontendSnapshotV1 {
  snapshot_schema_version: 1
  queued_prompts: [{ queue_ordinal, prompt_id, submission_id, content }]
  active_turn: { turn_id, prompt_id, status: active | interrupted } | null
  context: {
    context_revision
    context_manifest_id
    items: [{ ordinal, source_event_id, source_identity, content_ref }]
  }
  conversation: [
    {
      item_ordinal: u64
      kind: committed_message | assistant_evidence
      turn_id: uuid | null
      step_id: uuid | null
      request_id: uuid | null
      message_id: uuid | null
      response_attempt_ordinal: u32 | null
      content_kind: text | thinking | null
      chunk_ordinal: u32 | null
      content_ref: ContentRef | null
      transcript_message: TranscriptMessageV1 | null
      status: committed | partial | abandoned | abandoned_after_commit
      source_event: { sequence, event_id }
    }
  ]
}
```

Queue order is FIFO reducer order. Context order is manifest order and the whole
source or target revision changes atomically after `compaction.applied`.
Conversation order is source-event sequence followed by authority ordinal.
`committed_message` requires a non-null transcript message and null chunk-only
fields. `assistant_evidence` requires null transcript message and identifies one
durable assistant chunk. Chunks belonging to an open attempt are `partial`;
chunks terminalized without a message commit are `abandoned`; committed chunks
are represented only through their committed message. A committed message whose
owner is later abandoned is `abandoned_after_commit`. Prepared-but-unjoined and
joined-open requests, prepared-only invocations, failed attempts with no durable
display chunk, and unresolved compaction remain reducer state but do not invent
conversation content. Live tool progress is not durable authority and must be
joined by each downstream frontend as an ephemeral overlay, never serialized in
this snapshot or used on replay.

### Compaction-checkpoint DTO

The bounded single `CompactionCheckpointV1` is:

```text
CompactionCheckpointV1 {
  checkpoint_schema_version: 1
  context_revision
  context_manifest_id
  context_items: [{ ordinal, source_event_id, source_identity, content_ref }]
  compaction_state: never | idle | in_progress | applied | abandoned
  active_compaction: {
    compaction_id
    owner_scope
    source_frontier
    source_context_revision
    target_context_revision
    input_manifest_id
  } | null
  last_terminal: {
    compaction_id
    terminal: applied | abandoned
    terminal_event: { sequence, event_id }
    source_context_revision
    target_context_revision: string | null
    replacement_manifest_id: string | null
    compaction_summary_id: uuid | null
    reason_code: string | null
  } | null
}
```

`never` means no compaction fact exists; `idle` means compaction history exists
but no active operation and the latest terminal is represented by
`last_terminal`; `in_progress` requires `active_compaction`; `applied` and
`abandoned` describe a terminal at the exact source frontier and require the
matching `last_terminal`. A subsequent ordinary authority event changes the
state to `idle` without losing that terminal. Applied context fields are the
exact replacement manifest; an active or abandoned compaction retains the
source context. Restricted continuity never enters checkpoint fields.

### Determinism, coordination, and failure

Arrays use the semantic orders specified above; object member order, number and
string encoding, and escaping use RFC 8785 canonical JSON. Published bytes are
UTF-8 with no BOM, insignificant whitespace, or trailing newline. Content and
output digests cover those exact bytes. Wall-clock time, filesystem order,
hash-map order, wakeup count, and process-local identity are forbidden inputs.
Replaying the same session/stream/frontier and versions must produce byte-for-
byte identical chunks, envelopes, manifests, and cursor digests.

One session publication coordinator receives authority-append and recovery-
complete wakeups. It waits up to 50 ms to coalesce a burst, but a step, turn, or
compaction terminal requests immediate publication; continuous input may delay
publication by at most 250 ms. Before each run it captures the latest stable
read-only replay frontier and runs all four projectors against that same
frontier. A newer wakeup during work sets one dirty bit; after the run the
coordinator replays the then-latest stable frontier rather than every
intermediate wakeup. Thus coalescing may skip derived revisions but never leaves
the latest stable frontier unreplayed. Each projector has its own generic cursor
identity, lock, output revision, chunks, and failure result.

The production owner is a capacity-one, dirty-bit worker held by the durable
session supervisor. Authority code signals it only after a successful durable
append and never waits for replay or publication. Startup after recovery sends
an immediate catch-up hint. Shutdown clears and joins owned workers. Atomic
replacement clears and stops the old notifier, fences its adjacent
session-specific root, and transfers the join handle to the replacement
supervisor for owned reaping without delaying host publication.
Sessionless supervisors create no worker. Typed worker state exposes the last
strict-replayed sequence plus replay/coordinator and per-projector failures for
logs and tests; those failures preserve earlier committed outputs.

The four publications are independent. A build, bound, I/O, sync, rename,
digest, cursor, or validation failure leaves that projector's prior output and
cursor as the only committed frontier, removes only safe temporary files, and
does not block peer projectors. A crash after chunk/output publication but before
cursor publication follows generic cursor-v1 recovery and never serves the new
bytes under the old cursor. No projector appends authority, invokes a provider
or tool, reads restricted content, or falls back to mutable compatibility state.

At the task-5.3 boundary, publication was shadow-only. It wrote and validated these internal projections and
goldens, but `ConversationState`, provider dispatch, transcript/export commands,
TUI, ACP, Web, IPC, whole-file session snapshots, metadata checkpoints,
narrative journals, audit consumers, and compaction compatibility consumers do
not read them. Task 5.4 now migrates those consumers while retaining the
rollback mirrors through 5.6.

### Task-5.3 fixture and golden matrix

Task 5.3 extends the existing fixture corpus with byte-exact canonical JSON,
chunk, manifest, envelope, and cursor goldens for all four projectors:

| Family | Required assertions |
| --- | --- |
| lineage | legacy availability-only; mixed exact suffix and unavailable full export; full exact output; boundary item exclusion |
| provider requests | initial request; context/history repair as distinct inputs; joined open then closed; prepared-unjoined exclusion; retry does not change input; continuity IDs without restricted bytes |
| transcript | prompt/assistant/tool-result commitment; zero-text tool-call commit; failed-attempt exclusion; partial exclusion; committed-then-abandoned abnormal visibility |
| frontend | FIFO queue; active/interrupted turn; atomic context revision; durable partial then commit; durable partial then abandonment; no tool-progress field |
| compaction | never, in-progress, applied, abandoned, subsequent idle; turn-owned and idle ownership; crash around apply; exact source/target context |
| bounds | 4,096-item and 8 MiB chunk edges; deterministic split; empty manifest; 16 MiB manifest/inline acceptance and overflow rejection |
| determinism | shuffled map/filesystem insertion; repeated replay; coalesced versus per-event wakeups; identical canonical bytes and SHA-256 digests |
| publication | chunk-before-manifest; output-before-cursor; new output/old cursor; stale output/new cursor; independent projector failure; restart and idempotent republish |
| authorization/failure | missing, substituted, cross-session, and restricted blobs; unsupported event; gap/truncation; prior stable frontier retained with no exact partial output |
| shadow boundary | source guard and integration assertion that the 5.3 boundary kept every named 5.4 consumer on its compatibility path |

Goldens pin every closed enum spelling and every field, including explicit nulls;
unknown, omitted required, reordered semantic items, or noncanonical bytes fail.
Task 5.5 applies the frozen lag, disconnect, corruption, and blob-loss campaign
across the implemented consumers without changing these schema-v1 vectors.

## Task-5.4.0 consumer migration freeze

This section is normative for task 5.4. It freezes authority roles, schemas,
cutovers, compatibility, and tests without changing runtime. All new JSON DTOs
use strict RFC 8785 canonical JSON, reject unknown fields, use checked integer
bounds before allocation, and inherit the UUID, digest, content-reference, sync,
atomic-publication, and parent-sync rules above.

### Authority-role matrix

| Concern | Owner | May authorize | Must never authorize |
| --- | --- | --- | --- |
| session/turn/message/tool/compaction facts | semantic authority stream | replay, exact semantic transcript, current semantic state | operator labels or presentation observations |
| provider request input | synchronous `CurrentContextViewV1` reduced from authority at the captured frontier | one request preparation and dispatch after digest/frontier binding | later requests, from provider-history output or a stale projector |
| provider-history publication | `session.provider-history` projection | immutable evidence of prior joined request inputs | provider dispatch or context repair |
| host intent and plans | `HostStateCheckpointV1` | restore operator-owned host workflow state | semantic counters, message history, exact transcript, terminal facts |
| operator observations | append-only `ObservationRecordV1` ledger | restore and attribute operator-originated observations | semantic tool/result facts unless separately materialized into authority |
| friendly name and description | operator metadata in `SessionCatalogMetadataV1` | catalog labeling | inferred semantic truth or automatic overwrite |
| counters and last-prompt summary | semantic catalog reducer | display and telemetry at its named frontier | operator metadata mutation or dispatch |
| frontend evidence and compaction status | task-5.3 projections | cursor-qualified display and recovery hints | semantic authority or exact-current dispatch when stale |
| telemetry | `SessionTelemetrySnapshotV1` | diagnostics at its named sources | replay, admission, or billing authority |
| audit | audit ledger v2 | security/diagnostic evidence under its own retention | semantic replay or transcript |
| narrative journal | Markdown entry v2 | human continuity with machine-readable provenance | replay, exactness, or deduced completion |
| legacy `.json`/`.meta.json` | compatibility mirror | labeled legacy/mixed resume and rollback display through 5.6 | full-spine authority, old-writer admission, exact full-session claims |

No row acquires another row's authority because it contains copied fields. Every
copy retains source kind and cursor. Sessionless semantic lineage is not defined
by this task and remains route-only.

### Synchronous current-context view

Provider dispatch uses no durable current-context projector. The authority owner
holds the session admission/writer coordination boundary, materializes any
model-visible host or generated input as an attributed semantic source, captures
the latest synced end-of-stream frontier, and calls strict read-only replay
synchronously. It returns this immutable closed DTO:

```text
CurrentContextViewV1 {
  view_schema_version: 1
  session_id
  stream_id
  source_frontier: { sequence, event_id }
  lineage_level: mixed | full
  exactness: exact_suffix | exact_full
  context_revision
  context_manifest_id
  items: [
    {
      ordinal: u32
      role: system | developer | user | assistant | tool
      source_kind: admitted_prompt | committed_assistant |
        recorded_tool_result | materialized_context | compaction_summary
      source_event: { sequence, event_id }
      source_identity
      owner_id
      owner_generation_id
      content_ref
    }
  ]
  schema_set_id
  schema_set
  canonical_digest_algorithm: sha256
  canonical_digest
}
```

Items use context-manifest ordinal with no secondary sort. The manifest itself is
the ordering decision: generated system/developer material, host-state material,
committed semantic history or applied compaction summary, and the current
admitted prompt appear exactly where context composition placed them. No reader
may regroup by role, timestamp, owner, or message kind. Every item has one source
event and generation provenance; legacy transcript bytes, observation-ledger
bytes, journal prose, audit previews, and unmaterialized host state are excluded.
If an `IntentDocument`, plan, or operator observation becomes model-visible, its
rendered bytes first receive a default-class content reference and a
`context.source_materialized` source event; the host checkpoint or observation
record remains its external provenance, not semantic authority.

The source frontier is fresh only when it equals synced authority EOF captured
under the coordination boundary after all input materialization. Freshness has
no time-to-live and is not inferred from wall time. `model.request_prepared`
binds the context revision, manifest, schema set, and canonical view digest; the
route join and dispatch must still belong to that request. Any intervening
authority transition that invalidates active-turn or execution identity fails
the request rather than mutating the view.

The view has at most 4,096 items and its canonical reference/schema DTO is at
most 16 MiB. Each content reference retains the existing 16 MiB blob limit;
provider/model context budget and schema limits may narrow those ceilings.
`canonical_digest` covers the RFC 8785 bytes of the view with the two digest
fields omitted, avoiding a self-referential digest.
Overflow, sequence gap, unsupported required fact, missing/substituted blob,
restricted-content authorization failure, unattributed post-boundary item,
noncontiguous ordinal, digest mismatch, or source-frontier mismatch fails before
provider dispatch. There is no provider-history, projector, compatibility
snapshot, or previous-view fallback.

### Host-state checkpoint v1

`IntentDocument` and plans leave the whole-file conversation snapshot and use a
separate replaceable checkpoint:

```text
HostStateCheckpointV1 {
  checkpoint_schema_version: 1
  session_id
  stream_id: uuid | null
  host_state_revision: u64
  source_frontier: { sequence, event_id } | null
  saved_at: diagnostic RFC3339 UTC
  intent: {
    current_task: string | null
    approach: string | null
    lifecycle_phase
    task_mode
    task_mode_pinned: bool
    guidance_state: {
      commit_nudged: bool
      skill_completion_nudged: bool
      plan_reconciliation_fingerprint: u64 | null
      plan_reconciliation_nudges: u8
      mcq_detected: bool
      obfuscation_detected: bool
      operator_correction_pending: bool
      pending_action: PendingActionV1 | null
      constraints_discovered: [string]
      failed_approaches: [FailedApproachV1]
      open_questions: [string]
    }
  }
  plans: {
    next_plan_index: u64
    visible_plan_id: string | null
    visible: HostPlanV1 | null
    retained: [HostPlanV1]
    completed: [CompletedHostPlanV1]
    registry_view: PlanRegistryViewV1
    events: [PlanEventV1]
    completion_ledger: [CompletionLedgerEntryV1]
  }
}

HostStateCursorV1 {
  cursor_schema_version: 1
  session_id
  stream_id: uuid | null
  host_state_revision: u64
  source_frontier: { sequence, event_id } | null
  checkpoint_digest_algorithm: sha256
  checkpoint_digest
}

HostPlanV1 {
  plan_id
  scope: session | repo
  source: ephemeral | design | openspec | branch | hybrid
  binding: PlanBindingV1
  mode: off | planning | approved | executing | complete
  items: [{ description, status, intent, completion_policy, evidence }]
}
```

`PendingActionV1`, `FailedApproachV1`, `PlanBindingV1`, task intent/completion
policy/evidence, completed-plan, registry-view, plan-event, and completion-ledger
shapes are the existing closed plan-domain DTOs captured under host-state schema
version 1; changing one incompatibly requires host-state schema version 2.
`guidance_state` excludes conversation content, files-read/files-
modified collections, evidence counters, turn/tool/token counters, route state,
and terminal state. `HostPlanV1` preserves the existing stable session-local plan
identity, source/binding, mode, ordered work items, statuses, evidence references,
and visible/backgrounded disposition. It contains at most 256 plans, 1,024 work
items total, and 4 MiB canonical bytes; strings retain their existing domain
bounds and otherwise are capped at 64 KiB. Revision starts at one and increments
for every committed host-state change. Output-before-cursor atomic replacement
applies, with the checkpoint itself carrying its source frontier and digest in a
generic host-state cursor.

The checkpoint may trail semantic authority and may be restored at its disclosed
frontier. Semantic counters and file/tool evidence are recomputed from semantic
facts and the observation ledger as appropriate; conflicting copied legacy
counters are ignored. A missing/corrupt checkpoint degrades host workflow state
to unavailable and does not block strict semantic replay. An unsupported
checkpoint version fails host-state restore without rewriting or downgrading it.

### Durable operator observation ledger v1

Operator-originated tool observations use one session-adjacent append-only JSONL
ledger, independently sequenced and synced before acknowledgement:

```text
ObservationRecordV1 {
  record_schema_version: 1
  observation_id: uuid
  session_id
  ledger_sequence: u64
  source_frontier: { stream_id, sequence, event_id } | null
  execution_id
  tool_name
  arguments_ref
  cwd
  result: {
    content_refs
    is_error: bool
    exit_code: i64
    duration_ms: u64
  }
  origin
  observed_at: diagnostic RFC3339 UTC
}
```

Ledger sequence is contiguous from one. `observation_id` and `execution_id`
deduplicate retries; conflicting reuse is corruption. Arguments and result
content use default-class content-addressed references and the existing per-blob
limit; a record is at most 1 MiB and contains at most 256 content references.
Observations are ordered only by ledger sequence. They do not create a prompt,
turn, tool result, transcript message, semantic counter, or provider input. A
model-visible observation must separately pass content admission and semantic
materialization as described above. Legacy inline observations migrate once by
deterministic identity and remain in the compatibility mirror during dual-write.

### Catalog and telemetry schemas

Operator metadata and semantic catalog facts are joined, not overwritten:

```text
SessionCatalogMetadataV1 {
  metadata_schema_version: 1
  session_id
  metadata_revision: u64
  workspace_identity
  friendly_name: string | null
  description: string | null
}

SessionCatalogProjectionV1 {
  projection_schema_version: 1
  session_id
  lineage_level
  semantic_frontier: { stream_id, sequence, event_id } | null
  metadata_revision: u64
  created_at: diagnostic timestamp | null
  turns: u64
  tool_calls: u64
  last_prompt_snippet: string | null
  resume_mode: legacy_compatibility | legacy_base_plus_exact_suffix | exact
  exact_full_session_export: bool
}
```

Friendly name and description change only through an explicit operator metadata
mutation. Empty legacy values migrate to null; generated defaults may be offered
to the operator but are not semantic derivations and never overwrite a non-null
operator value. Turns, tool calls, last prompt, lineage, and export capability
are reducer-derived at `semantic_frontier`; legacy-only values are labeled
compatibility estimates. Catalog joins reject mismatched session/workspace
identity and disclose independently stale metadata or semantic sources.

```text
SessionTelemetrySnapshotV1 {
  telemetry_schema_version: 1
  session_id
  semantic_frontier: { stream_id, sequence, event_id } | null
  observation_frontier: u64 | null
  derived: {
    turns
    requests
    tool_calls
    input_tokens
    output_tokens
    cache_read_tokens
    latest_route
    context_revision
  }
  observed: {
    context_window
    estimated_tokens
    context_class
    thinking_level
  }
  observed_source: legacy_checkpoint | runtime_telemetry | none
  observed_at: diagnostic RFC3339 UTC | null
}
```

Each derived field must be supported by facts at the named semantic frontier;
unsupported fields are null rather than copied from a legacy checkpoint.
Observed fields are non-authoritative telemetry with explicit source and may be
stale. Telemetry never gates resume, provider dispatch, or semantic recovery.

### Audit and Markdown journal ownership

Audit remains append-only JSONL under its existing independent retention and
rotation policy. New records use this envelope while legacy records remain
readable:

```text
AuditRecordV2 {
  audit_schema_version: 2
  audit_id: uuid
  recorded_at: diagnostic RFC3339 UTC
  session_id
  kind
  source: {
    kind: semantic_event | observation | host_state | runtime_observation
    stream_id: uuid | null
    sequence: u64 | null
    event_id: uuid | null
    observation_id: uuid | null
    host_state_revision: u64 | null
  }
  source_dedup_key
  data
}

AuditSourceCursorV1 {
  cursor_schema_version: 1
  session_id
  source_kind: semantic_event | observation | host_state
  stream_id: uuid | null
  sequence: u64 | null
  event_id: uuid | null
  observation_sequence: u64 | null
  host_state_revision: u64 | null
  last_source_dedup_key
}
```

For semantic-backed records, `source_dedup_key` is SHA-256 over session, stream,
sequence, event ID, and audit kind. For observation and host-state records it
uses their stable identity/revision plus kind. Runtime observations receive a
fresh audit ID and are not falsely deduplicated as semantic events. Cursor state
records the last consumed source frontier per source kind; restart resumes after
that cursor, and duplicate delivery is ignored only when the full dedup key
matches. Audit loss or corruption is reported but cannot block semantic append,
dispatch, or resume.

The journal remains Markdown. Every new entry starts with its existing human
heading followed immediately by one ASCII machine-readable provenance comment:

```markdown
<!-- omegon-journal-source-v1 {"catalog_metadata_revision":1,"exactness":"exact_full","host_state_revision":1,"lineage_level":"full","observation_frontier":1,"semantic_frontier":{"event_id":"...","sequence":1,"stream_id":"..."},"session_id":"..."} -->
```

The JSON object uses canonical member ordering and explicit nulls where a source
does not exist. `JournalEntryV2` is the existing Markdown heading, this required
provenance-comment schema v1, and the existing optional task, outcome, model,
OpenSpec, and commit sections in that order. Journal task/outcome prose, Git commits, OpenSpec status, timing,
and token summaries remain narrative observations. Readers may validate and
display provenance but never reconstruct authority or infer completion from the
journal. Old entries without the comment remain legacy narrative entries.

### Validated readers and consumer cutover

A projection reader first validates path confinement, schema/projector version,
session/stream identity, canonical bytes, output digest, cursor revision, exact
authority frontier, manifest/chunk bounds, chunk identities, and lineage
envelope. New output with an old cursor is uncommitted and ignored. New cursor
with absent/stale output is corruption. Unknown versions, impossible frontiers,
missing chunks, digest mismatch, or bound failure return a typed unavailable
result; readers never parse a nearby legacy file as the same projection.

| Consumer | Task-5.4 source | Stale/failure behavior |
| --- | --- | --- |
| provider dispatch/context repair | synchronous `CurrentContextViewV1` | exact captured EOF required; fail closed before dispatch; no fallback |
| exact resume | synchronous replay plus host-state/observation reads | exact frontier required; fail closed for full lineage; mixed/legacy behavior below |
| `/transcript` | `session.transcript` at requested exact frontier | wait for explicit flush or synchronously rebuild; never return stale as exact |
| `/session-export` | validated frontend snapshot plus disclosed overlays | may return stale/partial with cursor, lag, lineage, and exactness labels |
| TUI/ACP/IPC attach | validated frontend snapshot plus ephemeral live overlay | may show stale snapshot with event-count lag; actions recheck current authority |
| Web live | validated frontend snapshot plus live overlay | same disclosed stale behavior; no stale authority |
| Web historical | exact transcript publication | full lineage exact; mixed exact suffix only; legacy unavailable |
| compaction restore | validated compaction checkpoint, then strict replay | stale checkpoint catches up from authority; invalid checkpoint rebuilds; no legacy fallback after boundary |
| session catalog/list | catalog projection joined with operator metadata | may display stale labeled values; identity mismatch omits entry and diagnoses |
| telemetry/checkpoint readers | telemetry snapshot | fail open to unavailable diagnostics, never to authority |
| audit/journal | own ledgers with source cursors | fail open for runtime progress, diagnose loss, never repair authority |

UI lag is measured as `current_known_sequence - source_frontier.sequence`, not by
timestamp. If current sequence is unknown the UI labels freshness `unknown`.
Provider dispatch and exact resume never accept `unknown` or nonzero lag.

### Replacement publication set and commands

After the completed task 5.4 cutover, the supported session publications are the semantic authority and
blob store; the four task-5.3 projections and cursors; host-state checkpoint and
cursor; observation ledger; operator catalog metadata plus catalog projection;
telemetry snapshot; audit v2; and Markdown journal v2 entries. The compatibility
`.json` and `.meta.json` pair is a temporary mirror, not part of the replacement
authority set. Legacy turn-checkpoint JSONL remains readable during migration but
new telemetry writes target `SessionTelemetrySnapshotV1`.

`/transcript [file|open]` writes the exact committed semantic transcript and its
lineage/frontier header. It is unavailable for legacy sessions and for a mixed
full-session request; a mixed suffix request is explicitly labeled. The current
clean clickable/presentation/evidence behavior moves to
`/session-export [file|open|scrollback]` and includes exactness, source frontier,
staleness, durable partial/abandoned evidence, and ephemeral-overlay disclosure.
`/copy session` remains presentation copy and does not acquire transcript
exactness. ACP/CLI/Web command registries expose names only where the transport
can preserve these semantics; task 5.4 updates private command help and task 5.6
owns applicable public docs. No configuration key is added.

### Legacy, mixed, full, rollback, and dual-write

- Legacy resume remains a labeled compatibility load from `.json` plus host
  metadata. It makes no semantic transcript/provider-history claim.
- Mixed resume loads an immutable labeled legacy base, then appends only the
  verified exact semantic suffix. The join boundary and both source identities
  remain visible. For provider continuity, the base may be rendered only as a
  labeled compatibility context block, content-addressed, and newly bound by
  `context.source_materialized`; that event proves the exact bytes used now but
  does not convert the base into historical semantic messages or enable exact
  full-session export.
- Full resume derives conversation/context from authority and joins host state
  and observations only under their own roles. It never loads conversation bytes
  from the mirror.
- Sessionless hosts remain route-only. This task creates no synthetic session,
  stream, prompt, turn, or semantic suffix for them.
- Mixed Web historical output is exact suffix only. No Web/API renderer may
  concatenate the compatibility prefix and label it exact.

Task 5.6 closes compatibility publication at a semantic self-sufficiency
boundary. Full lineage and mixed lineage carrying exactly one durable
content-addressed `legacy-compatibility-base-v1` stop rewriting the legacy
`.json` and `.meta.json` pair. Legacy lineage and mixed lineage not yet carrying
that source retain the pair only as a one-way import source. Opening a valid pair
beside pre-boundary authority materializes the compatibility LLM view exactly
once and establishes mixed lineage before any full-spine step. Existing pair
artifacts are not automatically deleted, but missing or stale pair bytes cannot
alter context, transcript, or resume after materialization.

Maintenance inventory, inspect, and quarantine prefer the semantic catalog and
identity-pin its framing, with pair fallback for legacy sessions. The runtime has
no rollback consumer-source selector, so closeout does not claim or add one.
Semantic writing and forward-only lineage remain active; unsupported reduced
writers cannot append a post-boundary turn, and no compatibility artifact can
truncate authority, rewrite a cursor, reclassify lineage, or authorize exactness.

### Version and fixture strategy

Authority envelope/event v1, every existing event-v1 payload, reducer/cache v5,
generic projector cursor v1, and all four task-5.3 projection schema-v1 DTOs are
unchanged. `CurrentContextViewV1`, `HostStateCheckpointV1`, its cursor,
`ObservationRecordV1`, `SessionCatalogMetadataV1`,
`SessionCatalogProjectionV1`, `SessionTelemetrySnapshotV1`, `AuditRecordV2`, and
journal provenance v1 are new independently versioned contracts. A future
incompatible shape gets a new schema version and explicit migration; fields are
not silently added to a strict version. No old-writer downgrade or negotiated
minimum authority reader is supported.

Task 5.4 includes red-to-green fixtures for:

| Family | Required assertions |
| --- | --- |
| current context | semantic ordering/provenance; host materialization; exact EOF; projector lag independence; all bounds and fail-closed cases; digest binding through dispatch |
| host state | intent/plan round trip; revision/cursor; semantic-counter exclusion; stale restore; missing/corrupt/unsupported behavior; plan bounds |
| observations | append/sync; deterministic legacy migration; ID dedup/conflict; blob verification; no implicit semantic/provider input |
| readers | valid, stale, new-output/old-cursor, stale-output/new-cursor, wrong identity/version, missing chunk, digest/bound failure; disclosed UI lag |
| cutovers | provider, resume, transcript, session export, TUI, ACP, IPC, Web, compaction, catalog, telemetry, audit, and journal source guards |
| lineage | labeled legacy; mixed base plus exact suffix; full exact; mixed full-export denial; Web suffix-only; sessionless absence |
| audit/journal | semantic cursor and dedup across restart; distinct runtime observations; canonical journal provenance; old Markdown entry readability |
| compatibility | post-authority mirror ordering; mirror failure isolation; `.json`/`.meta.json` old-reader shape; rollback source switch; old-writer denial; no downgrade |
| commands/docs | `/transcript` exactness; `/session-export` presentation evidence; `/copy session` remains presentation; no config/site/snippet change in refinement |

Task 5.5 runs the frozen campaign below against these implemented contracts; it
may not weaken their fail-closed exactness or redefine schema versions.

## Task-5.5.0 adverse-consumer campaign freeze

This section is normative for task 5.5 and changes no runtime. Scenario IDs,
faults, dispositions, axes, consumer laws, and injection points are frozen. A
future semantic change adds a versioned campaign; it does not reinterpret an ID.

### Typed faults and dispositions

`ConsumerFaultV1` has exactly these spellings:

```text
notification_skipped | notification_lagged | consumer_disconnected |
consumer_restarted | output_missing | output_stale | output_malformed |
output_digest_mismatch | chunk_missing | chunk_digest_mismatch |
record_torn | identity_mismatch | authority_unavailable |
append_failed | sync_failed | rename_failed | worker_stopped |
mirror_publication_failed
```

`ConsumerDispositionV1` has exactly these spellings:

```text
current | caught_up | rebuilt | quarantined_rebuilt | degraded_stale |
degraded_unavailable | partial_publication | blocked_unavailable | blocked_corrupt |
semantic_source_unavailable | fatal_store_invariant
```

The vocabulary is an internal test/result contract, not a new persisted wire
schema. `blocked_unavailable`, `blocked_corrupt`, `semantic_source_unavailable`, and
`fatal_store_invariant` are distinct: malformed owned bytes are corruption, an
exact operation without valid required input is blocked unavailable, an existing
semantic source that cannot be read is unavailable, and a required store-set
relation that cannot be true is fatal. `partial_publication` means the
semantic operation is durable but one named non-authoritative publication
failed; it is never ordinary success or semantic rollback.

### Axes and consumer laws

| Axis | Closed values |
| --- | --- |
| lineage | `legacy`, `mixed`, `full` |
| lifecycle | `late_attach`, `lagged`, `disconnected`, `restarted`, `replacing`, `steady` |
| consumer | `exact`, `projection`, `frontend`, `host_record`, `evidence`, `mirror` |

Fault and platform are tagged dimensions. The manifest below covers every pair
of lineage, lifecycle, and consumer values at least once, and every fault is
crossed with a consumer whose law can distinguish it.

| Consumer law | Required behavior |
| --- | --- |
| exact: dispatch, exact resume, transcript, compaction restore | Validate authority, blobs, lineage, and every required owned store at the requested frontier; synchronously rebuild derived state or fail closed. Never use stale, damaged, journal, audit, or mirror bytes. |
| projection: four semantic projectors and validated readers | Serve only a valid committed cursor/output pair. Stale valid output may be labeled; missing/damaged output is unavailable and rebuildable. A proven corrupt owned chunk alone may be quarantined and deterministically replaced from validated authority under that projector's lock. |
| frontend: TUI, ACP, Web live, IPC | Display validated stale/unavailable state with frontier and event-count lag, but reconcile actions against current supervisor state. Completion and idle queue state are canonical; presentation loss cannot retain a busy gate. |
| host record: host state, observations, operator catalog | Host-state corruption fails restore. A missing observation ledger is empty/degraded only if no durable marker, host frontier, catalog field, or mirror provenance proves it existed; malformed/torn content fails closed. Existing authority plus missing catalog record is `fatal_store_invariant`. |
| evidence: telemetry, audit, journal | Failure cannot block authority progress. Malformed semantic audit input stops the semantic audit cursor at the prior valid row and warns; it emits no duplicate or quarantine row. Journal failure against existing authority is `semantic_source_unavailable`, never sessionless. |
| mirror: `.json`/`.meta.json` through 5.6 | Semantic durability occurs first. Mirror failure returns `partial_publication`, names the failed publication, preserves semantic resumability, and never enables old-writer downgrade or an exactness claim. |

Replacement validates target authority/blobs, host-state, observation policy, and
catalog identity before publication. The four derived projections are not
required host stores: missing or damaged projection state is published as
unavailable, fenced by the new generation, and rebuilt by its worker. No repair
may infer authority from another consumer.

ACP drains already-queued notifications only to the fixed worker shutdown/drain
deadline; expiry discards advisory backlog and immediately reconciles canonical
worker/supervisor completion and queue state. IPC receiving a lag indication
automatically enqueues exactly one current reconciled state for that connection
before accepting later deltas; it does not require a client retry and may
coalesce superseded reconciliations.

### Frozen 54-scenario manifest

Each ID is one required fixture/harness assertion. Rows are intentionally terse;
the axes and consumer laws above supply the full oracle.

| IDs | Consumer | Frozen scenarios in ID order | Expected dispositions in ID order |
| --- | --- | --- | --- |
| `AC01`-`AC09` | exact | full/late attach catches up exactly; full/lag ignores stale projection and synchronously reduces; full/disconnect with authority unavailable fails as semantic source unavailable; mixed/restart restores labeled base plus exact suffix; mixed/replacement validates host stores and publishes despite disclosed projection damage; mixed/steady missing required blob is blocked; legacy/late attach is labeled compatibility; legacy/restart offered mirror-only exactness is denied; legacy/steady authority append failure retains the prior compatibility state without claiming success | `caught_up`; `rebuilt`; `semantic_source_unavailable`; `caught_up`; `degraded_unavailable`; `blocked_corrupt`; `degraded_unavailable`; `blocked_unavailable`; `blocked_unavailable` |
| `AC10`-`AC18` | projection | full/restart chunk digest mismatch is quarantined/rebuilt; full/replacement malformed derived output publishes unavailable then rebuilds; full/steady output digest mismatch or new-output/old-cursor rebuilds; mixed/late missing chunk rebuilds exact suffix; mixed/lag stale valid cursor is labeled stale; mixed/disconnect stopped worker catches up at startup; legacy/lag projection remains unavailable; legacy/disconnect missing output remains unavailable; legacy/replacement worker remains absent | `quarantined_rebuilt`; `rebuilt`; `rebuilt`; `rebuilt`; `degraded_stale`; `caught_up`; `degraded_unavailable`; `degraded_unavailable`; `degraded_unavailable` |
| `AC19`-`AC27` | frontend | full/lag IPC automatically enqueues reconciled state before delta; full/disconnect TUI current snapshot clears busy; full/steady ACP skipped terminal reaches idle and accepts second submit; mixed/late Web discloses suffix and staleness; mixed/restart ACP backlog uses bounded drain then reconciles; mixed/replacement generation fence rejects retired-worker update; legacy/late TUI has no false busy; legacy/restart ACP is labeled compatibility idle; legacy/steady IPC emits labeled compatibility state | `caught_up`; `current`; `caught_up`; `degraded_stale`; `caught_up`; `current`; `degraded_unavailable`; `current`; `current` |
| `AC28`-`AC36` | host record | full/restart absent never-created observation ledger degrades empty; full/replacement authority with missing catalog is fatal; full/late wrong catalog identity is fatal; mixed/lag torn observation row is blocked; mixed/steady absent ledger with durable existence evidence is blocked; mixed/disconnect conflicting observation ID is blocked; legacy/lag absent ledger with no existence evidence is labeled empty; legacy/disconnect missing host checkpoint is compatibility; legacy/replacement missing catalog without authority omits compatibility entry | `degraded_unavailable`; `fatal_store_invariant`; `fatal_store_invariant`; `blocked_corrupt`; `blocked_unavailable`; `blocked_corrupt`; `degraded_unavailable`; `degraded_unavailable`; `degraded_unavailable` |
| `AC37`-`AC45` | evidence | full/disconnect journal authority read failure is semantic source unavailable; full/restart malformed semantic audit row stops cursor and warns; full/late telemetry unavailable is diagnostic only; mixed/steady duplicate semantic audit delivery deduplicates only; mixed/lag journal provenance gap is labeled; mixed/replacement evidence failure cannot block replacement; legacy/steady malformed nonsemantic audit row warns without authority effect; legacy/late journal without authority remains legacy narrative; legacy/restart evidence absence fabricates no semantic source | `semantic_source_unavailable`; `blocked_corrupt`; `degraded_unavailable`; `current`; `degraded_unavailable`; `degraded_unavailable`; `degraded_unavailable`; `current`; `degraded_unavailable` |
| `AC46`-`AC54` | mirror | full/steady semantic save plus mirror rename failure returns partial publication; full/lag stale mirror cannot satisfy exactness; full/replacement mirror failure preserves semantic target; mixed/disconnect mirror sync failure returns partial publication; mixed/restart missing mirror leaves semantic suffix resumable; mixed/late mirror identity mismatch rejects only the mirror; legacy/disconnect mirror failure reports compatibility publication failure; legacy/lag stale mirror is labeled compatibility; legacy/replacement never admits an old writer | `partial_publication`; `degraded_stale`; `partial_publication`; `partial_publication`; `degraded_unavailable`; `partial_publication`; `partial_publication`; `degraded_stale`; `blocked_unavailable` |

### Sandbox, injection, platform, and budget

Every scenario copies immutable corpus inputs into a unique temporary sandbox;
authority, blobs, chunks, cursors, host stores, ledgers, catalog, journal, audit,
and mirrors are path-confined beneath it. The harness records before/after file
digests and rejects writes to source fixtures or outside the sandbox. IDs and
fixture bytes are deterministic; clocks, random scheduling, network, provider
calls, and wall-time sleeps are forbidden.

Fault injection occurs only at named boundaries: authority/ledger append,
`sync_all`, temporary-output write, atomic rename, parent sync, validated read,
notification enqueue/dequeue, worker start/stop/drain, generation-fence publish,
and mirror publish. Barriers/latches select exact operations and occurrence
numbers. Production-like adapters run unchanged behind those traits; tests do
not edit files concurrently to approximate a torn write.

The required matrix runs the same 54 IDs on Linux, macOS, and Windows. It uses
portable path confinement and atomic replace semantics and additionally checks
Windows sharing/rename refusal and macOS/Linux parent-sync behavior where the
platform exposes them. Unsupported crash-durability probes are reported as
supplemental, never silently passed. The required campaign must finish in at
most 15 seconds per platform in CI, use no retry-on-failure, and print the first
scenario ID, injection boundary, fault, disposition, and changed paths on
failure. Slow real-crash/filesystem suites are optional release evidence and do
not weaken this gate.

### Operator agency and exact red gaps

Every frontend completion case asserts all three invariants: canonical
supervisor/worker completion clears local active/streaming state without an
advisory terminal, an authoritative idle queue snapshot repairs missed terminal
delivery, and a second submission succeeds. Notification ordering, projection
damage, evidence failure, and mirror partial publication may not strand the
operator or authorize work from stale state.

The checked manifest has no expected-pending rows and every frozen ID now enters
a consumer-specific campaign oracle. The macOS campaign is within the required
budget, and focused tests cover corrupt derived chunk quarantine/rebuild,
replacement with damaged projections, skipped ACP completion, automatic IPC lag
reconciliation, observation-ledger existence proof, missing catalog fatality,
malformed semantic audit advancement, journal semantic-source unavailability,
and semantic/mirror partial publication. Task 5.5 is not yet complete: AC13 must
use a chunk-bearing mixed-lineage fixture rather than the broader
missing-derived-state fallback; several exact, adapter-specific frontend,
legacy-host, evidence-replacement, and mirror rows still reuse shared law probes
instead of enacting their frozen interaction; and the identical campaign still
needs Linux and Windows evidence. Supplemental platform-specific
crash-durability probes remain release evidence rather than weakening the
portable deterministic gate.
Task 5.5 may not rewrite accepted corpus bytes or alter event v1, reducer/cache
v5, cursor v1, projection v1, or task-5.4 store schemas.

Task 5.6 alone may remove compatibility dual-write or change developer and
applicable public session/resume/migration/recovery docs, public site pages, or
canonical snippets. Task 5.5 may update only private implementation-facing
documentation needed to keep this campaign truthful.

### Recovery and atomic replacement

Full recovery holds the writer and supervisor admission gates. It validates the
entire authority prefix and referenced blobs before appending. Its order is:

1. finish any previously committed-but-unpublished context replacement;
2. for an open compaction with a committed summary, append deterministic
   `compaction.request_closed(summary_committed)` if absent, append deterministic
   `compaction.applied` if absent, and atomically install its target revision;
3. otherwise close an open compaction request as `abandoned` and append
   deterministic `compaction.abandoned` for the open compaction;
4. classify dispatched/acknowledged invocations, preserving prepared-only
   invocations as incomplete;
5. close an open model request, abandon its step, and close its turn under the
   existing Slice-5.1 order;
6. rebuild reducer state and only then permit projections or new admission.

Each recovery append uses reason-bound UUIDv5 command/event identities and a
fixed recovery-rule version. A crash after any prefix resumes at the first
missing action. Repeating recovery is a no-op. Missing/tampered referenced blobs
stop before append or replacement; recovery does not abandon valid facts merely
to route around unavailable content.

Idle full-spine activation, reducer/snapshot replacement, and idle compaction
context replacement occur only while no turn, step, provider request,
compaction, or unresolved invocation is active. The session authority owner and
supervisor projection switch as one publication under their shared gate. A
concurrent prompt sees either the complete old state before the gate or the
complete new state after publication; it cannot bind a mixed reducer, cursor,
execution pair, or context revision.

The bounded host-session replacement component now applies that rule to
interactive `/resume`, `/new`, and `/context clear`, ACP load/new/control, and
daemon control. It rejects process-local or durable queued prompts rather than
guessing response ownership. Target authority recovery, referenced blobs,
read-only replay frontier, boot execution binding, and projection identity are
validated before one infallible publication replaces the displayed session ID,
writer/supervisor, compatibility conversation, resume metadata, and host
generation. `/context clear` and `/new` both allocate a fresh authority lineage;
neither erases semantic authority in place. Sessionless compatibility resets do
not acquire semantic authority and retain their prior behavior. This component
does not activate any concrete Task-5.3 projector.

### Snapshot/reducer version plan

Task 5.2 has introduced `snapshot_version: 5` and `reducer_version: 5` for the
foundational lineage fields. Version 5 retains all version-4 state and the
complete frozen plan adds:

```text
lineage_level: legacy | mixed | full
full_spine_boundary: { sequence, event_id } | null
response_attempt_terminals
materialized_context_sources
active_compaction | null
compaction request/attempt/summary/terminal indexes
context_revision and replacement manifest indexes
```

Version-4 caches are discarded and rebuilt from authority. The implemented v5
reducer accepts all supported baseline/v4 facts, derives
`legacy_only`/`mixed`/`full_spine`, records the first `step.started` or
`context.source_materialized` or `compaction.started` boundary, and never invents
absent state. It includes response-attempt terminals, materialized-source,
active/request/attempt/summary/terminal compaction, context-revision, and
replacement-manifest indexes. A v4 reader encountering any new required event
fails closed.
No snapshot upgrader rewrites v4 bytes or authority events. Projector cursors
have independent versions and are not embedded in the authority cache.

### Canonical fixture corpus

Tasks 5.2-5.5 must share one checked-in, byte-stable corpus under
`core/crates/omegon/tests/fixtures/session-semantic-v1/`. The corpus contains
authority JSONL, blobs and metadata, expected reducer v5 state, expected
read-only cursors, expected projector outputs/cursors, and a manifest of the
first expected failure for invalid cases. Required fixture families are:

- `legacy-no-authority`, `legacy-authority`, `mixed-boundary`, and `full-spine`;
- single request, same-step context/history repair, failed-attempt retry,
  zero-text tool-call commit, empty Done rejection, EOF, and cancellation;
- prepared-only invocation, dispatched unknown, denied call, and late settlement;
- turn-owned compaction success/failure/recovery and idle session compaction
  success/failure/recovery, including crash before and after apply;
- source-materialized provenance and forbidden post-boundary owner-only
  provenance;
- missing, substituted, cross-session, projection-class-mismatched, and tampered
  blobs;
- stale cache, stale output/new output-old cursor, new cursor-old output,
  duplicate delivery, sequence gap, unsupported event, truncated final line,
  lagged/disconnected restart, and repeated recovery.

Fixtures are append-only once consumed by an implementation. A semantic change
adds a versioned fixture; it does not rewrite an accepted vector. Task 5.2 owns
strict readers/reducer/replay vectors. Task 5.3 owns exact projection and
compaction-checkpoint vectors. Task 5.4 adds legacy-consumer migration vectors.
Task 5.5 runs corruption, lag, disconnect, restart, idempotency, and blob-loss
recipes across all applicable consumers.

## Snapshot and reducer versions

The currently implemented cache is version 5 with foundational lineage state;
the remaining frozen version-5 indexes are delivered with their owning event
families:

```text
snapshot_version: 5
reducer_version: 5
session_id
stream_id
last_sequence
  last_event_id
  lineage_level
  full_spine_boundary | null
state:
  workspace_identity
  runtime_generation_id
  execution_binding_generation | null
  submission, prompt, and turn identity indexes
  queued_prompts[]
  active_turn | null
  route_leases[]
  step and request identity/ordinal indexes
  active_step | null
  request-to-route joins
  context and schema manifests
  default assistant chunk and committed-message indexes
  restricted continuity fact metadata (never bytes or default indexes)
  canonical tool-call/result identity, ordinal, and linkage indexes
  terminal step indexes
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
- execution-binding migration is idle-only, rejects every nonterminal or
  unknown invocation, and changes the driver/route-service pair atomically.
- canonical calls/results are contiguous, one-to-one, content-validated, and
  cannot contradict invocation identity or lease state;
- a normal terminal step has no open request, missing result, or nonterminal
  admitted invocation, and its outcome controls next-step/turn continuation.

Version 4 retains the version-3 state plus the bounded step/request identities
and ordinals, request-to-route joins, context/schema manifests, assistant and
continuity facts, canonical tool calls/results, and terminal steps implemented
by these bounded components. Restricted blob bytes are never embedded in the
snapshot. Snapshot v3 is discarded and rebuilt from the authority stream;
it cannot advance across unknown required facts. The v4 reducer accepts all
supported older required events and preserves absent Slice-5 state for legacy
lineages. Task 5.2 replaces it with the frozen version-5 lineage, response-
attempt, provenance, compaction, and context-revision state above rather than
overloading v4. Provider-history, transcript, frontend, and compaction-checkpoint
projector state remains outside the authority snapshot.

## Recovery

The following paragraphs describe the currently implemented Slice-1 and
Slice-5.1 minimum paths plus the first bounded Slice-5.2 response-attempt and
crash-prefix hardening. Recovery derives its remaining classification and latest
attempt from durable lineage, including prior recovery classifications, failure
facts, chunks, continuity, and message commit. Once reducer v5 is active, the
remaining task-5.2.0 recovery and atomic-replacement order above extends these
paths for compaction without changing invocation/request/step/turn ordering.

Recovery holds the writer lease. Before Slice-5.1 facts, for an open turn it:

1. appends one deterministic `invocation.classified_unknown` for each registered
   unsettled invocation;
2. appends one deterministic `turn.closed(interrupted)`;
3. reconstructs and publishes the recovered snapshot.

Repeated recovery recognizes the deterministic identities and appends nothing.
Late loop, provider, EOF, or process-exit observations cannot append another
closure. Once the interrupted closure is durable, the next queued prompt may
start without waiting for a missed broadcast.

For a Slice-5.1 open step, minimum recovery instead appends in this order:

1. one deterministic `invocation.classified_unknown` for each eligible
   dispatched or acknowledged unsettled invocation under the existing Slice-3
   ordering, while preserving prepared-but-unhanded-off state;
2. one deterministic `model.request_closed(abandoned)` for the open request, if
   any;
3. one deterministic `step.abandoned` for the open step;
4. the existing deterministic `turn.closed(interrupted)`;
5. the recovered snapshot, followed by advisory recovery projection.

Recovery does not fabricate assistant commits, continuity, tool progress, tool
results, or normal step closure. Canonical calls without results remain visibly
unresolved and derive safety from their linked invocation state. Every recovery
append uses UUIDv5 command/event identities and fingerprints over the fixed
recovery rule and target identity, so a crash between any two appends resumes at
the first missing transition. Provider EOF and error paths close requests with
their strongest live outcome. Cancellation, timeout, revocation, worker failure,
host exit, and runtime loss then use the deterministic step-abandonment primitive
when a semantic step remains open. Host paths invoke it after owned
provider/tool cleanup and before the supervisor commits the narrowed turn
closure; dropping the loop future or emitting advisory terminal events is not
semantic terminalization.

## Compatibility

- Readers accept only envelope version 1 in Slice 1.
- Each `(event_type, event_version)` has immutable field and reducer semantics.
- Required-field, enum, validation, or reducer changes require a new event
  version; meaning is never changed retrospectively.
- Unknown authority events or versions fail replay. They are never silently
  skipped or defaulted.
- Snapshot caches may be discarded and rebuilt; authority events may not be
  renumbered, rewritten, or synthesized from legacy transcripts.
- Snapshot/reducer version 3 adds the optional reduced execution-binding
  generation. Absence remains meaningful for legacy streams and does not cause
  a synthetic migration on replay or resume.
- Snapshot/reducer version 4 is the minimum Slice-5.1 validation and abandonment
  cache. It does not claim complete replay-derived consumers. Older readers fail
  closed on new required events. Once the first full-spine event is durable, the
  writer cannot downgrade the lineage for an older reader.
- Legacy sessions may continue through the existing compatibility resume path,
  but their historical conversation snapshots are not represented as if they
  had always emitted semantic facts. Starting durable authority creates an
  explicit new stream lineage at the migration boundary.
- An existing authority lineage may begin Slice-5.1 emission only at the next
  `step.started` after a durable closed-step or turn boundary. No step, request,
  assistant message, tool call/result, content reference, or restricted
  continuity is synthesized for earlier events. Legacy whole-file and route-only
  sessionless streams retain their current behavior until their explicit
  migration tasks.

## Slice ownership

Slice 1 owns supervisor state, ordering, baseline compatibility, deterministic
runtime-loss closure, and conservative unknown invocation classification.

Slice 3 owns policy combination, leases, `Prepared`/`Dispatched`/`Acknowledged`
invocation durability, deduplication, unknown-completion fencing, and safe late
settlement.

Slice 4.5 owns boot selection, per-turn capture, in-memory pending intent, and
explicit quiescent migration of the atomic loop-driver and
provider-route-service generation pair. Migration publication follows durable
append, unknown completion blocks it, and resume boot binding remains
process-local. It does not add complete message, context, tool-result,
continuation, or step persistence.

Slice 4.2 owns the minimum pre-dispatch route lease, selected-versus-serving
identity, contribution-generation evidence, and session-backed versus
sessionless durability split. It does not make inventory diagnostics a dispatch
gate or add complete semantic step persistence.

Slice 5.0 freezes the event names, payloads, identities, cardinality, ordering,
content-reference security, restricted continuity, version strategy, and task
boundaries above. Slice 5.1 emits those facts for authority-backed sessions and
adds only the minimum deterministic request/step abandonment needed for crash
safety. It does not emit tool progress or sessionless full semantic streams.

Slice 5.2.0 freezes the compatibility matrix, new required event names/payloads,
response-attempt and provenance transitions, read-only replay, compaction
recovery, cursor schema, version-5 reducer plan, and canonical corpus. It changes
no runtime behavior and no existing event v1 payload.

Slice 5.2 implements strict full-spine decoding/reduction, lineage transition,
response-attempt validation, event-backed source provenance, read-only replay,
complete reducer/cache v5 indexes, compaction authority/recovery, atomic host
session replacement, and generic cursor validation/storage. The cursor substrate
validates replay frontiers and restrictive derived storage, then durably
publishes deterministic output before its strict cursor without activating a
concrete projector or consumer. Compaction emits and recovers the frozen facts
for exact event-backed manifests without activating derived provider-history,
transcript, frontend, or compaction-checkpoint consumers or changing compaction
policy. Slice 5.3 derives those concrete projections from existing authority
with output-before-cursor publication; it does not re-emit materialized-source or
compaction facts. Slice 5.4.0 freezes the authority-role matrix, synchronous
exact-frontier current-context view, separate host-state and observation stores,
catalog/telemetry/audit/journal schemas, validated readers, consumer cutovers,
publication replacement, dual-write rollback, command names, fixtures, and
version strategy without runtime changes. Slice 5.4 now migrates the named consumers
under that freeze. Sessionless semantic lineage remains deferred rather than
being implicitly approved by this slice.
Slice 5.5 completed late, lagged, disconnected, restarted, corrupted-consumer,
blob-loss, idempotency, and cursor recovery execution against the canonical
corpus without redefining Slice-1 identity, sequence, required-event
compatibility, an existing v1 payload, or turn closure.
