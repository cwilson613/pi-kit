# Provider routing: route contributions and leases - Delta Spec

## ADDED Requirements

### Requirement: Provider contributions bind complete route semantics

A provider contribution must bind provider identity, model inventory authority, authentication class, tool-schema dialect, bridge factory, modality/capability evidence, and explicit fallback compatibility under one stable owner.

#### Scenario: Provider contribution is incomplete
Given a candidate provider declares a bridge factory but no schema dialect or authentication class
When contribution validation runs
Then the provider route is ineligible
And diagnostics identify the missing route semantics

### Requirement: Every inference request captures a route lease

Before provider dispatch, the runtime must capture and durably associate a route lease containing provider identity, model identity, schema dialect, credential-source class, fallback reason, contribution generation, and route policy with the owning turn or step.

The existing `route.lease_recorded` v1 event and payload are immutable. For an authority-backed Slice-5 request, a new `model.request_route_joined` v1 fact must link exactly one request and step to exactly one previously appended route lease. A repaired context/history request in the same step must use a new request identity and a new route lease; a route retry that does not alter the prepared request or serving route retains the joined lease. Sessionless route evidence remains the Slice-4 step wrapper until a later full semantic-stream design.

Every transport attempt under a joined request has a contiguous response-attempt ordinal. Durable attempt-failure evidence must precede a retry. Provider Done alone may commit a message; EOF closes the request as EOF, a terminal provider error closes it as provider-failed when known, and cancellation or host timeout leaves the strongest truthful request outcome for host semantic terminalization after provider cleanup. No path may broadcast an authority-backed assistant chunk before its append succeeds.

Task 5.2 implements `model.response_attempt_failed` v1 as the required retry boundary when prepared request bytes and serving route remain unchanged. A final attempt closes through `model.request_closed` instead and cannot also authorize retry. Provider Done with no text or thinking may commit only when it carries one or more canonical tool calls; Done with neither content nor calls is not successful response evidence.

Turn-owned compaction continues to use the unchanged `route.lease_recorded` v1 payload, with its compaction request identity in `request_id`, and claims that lease through `compaction.request_prepared`. Manual idle compaction cannot fabricate a turn for that event. Its session-scoped `compaction.request_prepared` instead embeds the same selected/serving, schema, credential, fallback, contribution-generation, and policy evidence before dispatch. That evidence is scoped to the compaction and is not a general route lease or a sessionless route-wrapper record.

#### Scenario: Fallback route serves a request
Given the selected direct route is unavailable
And policy permits one compatible fallback
When the request is dispatched through that fallback
Then the route lease records selected and fallback identities plus the bounded reason
And later projections do not present the fallback as the originally selected route

#### Scenario: Context repair redispatches within one loop iteration
Given an authority-backed request fails with context overflow
When the loop repairs model history and dispatches again in the same internal iteration
Then the second request retains the durable step identity
And it receives a new request identity, route lease, and request-to-lease join
And `route.lease_recorded` v1 is not widened or reinterpreted

#### Scenario: Provider stream ends without Done
Given an authority-backed joined request has emitted partial durable content
When the provider stream reaches EOF without Done
Then the final pending chunk is durable before any projection
And the request closes as EOF with its response-attempt ordinal
And no assistant message commit is fabricated

#### Scenario: Same-route transport retry begins
Given a joined request retains identical prepared bytes and serving route
And its current transport attempt fails retryably
When policy starts another bridge call
Then `model.response_attempt_failed` is durable for the current ordinal first
And the next response-attempt ordinal is contiguous
And no failed-attempt content enters a later committed message

#### Scenario: Idle compaction resolves a provider route
Given a full-spine session is idle and manual compaction is admitted
When its provider route is prepared
Then session-scoped compaction evidence is durable before dispatch
And no prompt, turn, step, or turn-owned route lease is invented
And `route.lease_recorded` v1 remains unchanged

### Requirement: One route authority serves every runtime host

Interactive, daemon, child-agent, and bounded execution must resolve provider routes through one typed route-service contract and record the same route-lease shape. Host adapters must not construct provider bridges or fallback chains independently.

#### Scenario: Daemon and interactive sessions select the same route policy
Given daemon and interactive sessions use the same profile, model intent, credentials, and provider health snapshot
When each resolves its next inference request
Then both use the same route authority and policy inputs
And any different result is attributable to recorded session or timing evidence rather than host-specific routing code

### Requirement: Provider continuity is explicitly bounded

An executable provider contribution must expose a generation-bound continuity policy of either `none` or `restricted_required`. `restricted_required` must declare a non-empty subset of `hidden_reasoning` and `opaque_provider_state` plus a maximum blob size no greater than the session protocol's 16 MiB ceiling. Only the captured serving adapter may emit a declared kind, and only when those exact bytes are required to continue a later request on the same serving provider/model/generation. The policy grants no arbitrary raw-payload persistence or default-projection visibility and is not added to `route.lease_recorded` v1.

#### Scenario: Provider emits undeclared opaque state
Given the captured serving contribution declares continuity policy `none`
When its transport exposes an opaque response payload
Then no `provider.continuity_stored` fact is admitted
And the payload is neither persisted as a catch-all blob nor exposed through a default projection

### Requirement: Fallback cannot broaden silently

Provider contributions and adapters must not infer arbitrary cross-family fallback. Fallback compatibility must be declared and narrowed by current route policy and admission.

#### Scenario: Undeclared model-family substitution is proposed
Given a provider candidate can technically accept an OpenAI-compatible request
But no fallback compatibility relation exists for the selected model family
When route resolution runs
Then the candidate is not selected as fallback

### Requirement: Driver replacement is quiescent

The selected loop driver and provider route service may be replaced only at boot or a durably recorded quiescent session migration boundary, never during an active turn.

The durable migration target is one atomic pair of validated driver and provider-route-service contribution-generation IDs. It is distinct from the legacy runtime generation, composition generation, and request route-lease generation. Migration requires an idle session, an exact current source binding, and no unresolved registered, prepared, dispatched, acknowledged, or unknown invocation. Boot binding on resume is process-local and does not fabricate migration history for legacy streams. Mid-turn pending intent belongs to the in-memory session execution owner and is not a durable event.

#### Scenario: Replacement is requested mid-turn
Given a session has an active turn
When configuration requests replacement of its loop driver or route service
Then the atomic replacement pair remains in-memory Pending
And the active turn retains its captured driver and route generations
And neither turn closure nor the next turn start applies it
And only a deliberate caller's explicit quiescent commit may apply it

#### Scenario: An unresolved unknown invocation exists
Given a session is idle after a turn with a durable unresolved unknown invocation
When execution-binding migration is requested
Then migration is rejected
And no migration fact is appended

#### Scenario: A legacy session resumes under a boot binding
Given a legacy authority stream has no execution-binding migration fact
When the session resumes with a process-local driver and route-service binding
Then replay retains an absent durable execution binding
And resume does not append or infer migration history

### Requirement: Loop policy depends only on typed runtime contracts

The release-coupled loop driver must depend on typed session-transition, route-lease, context-assembly, and privileged-invocation contracts rather than concrete provider, tool, memory, lifecycle, or frontend implementations.

#### Scenario: Optional lifecycle service is absent
Given a product profile omits lifecycle services
When the loop executes a turn that does not require lifecycle capability
Then the loop operates through its typed contracts without importing or branching on a lifecycle implementation name
