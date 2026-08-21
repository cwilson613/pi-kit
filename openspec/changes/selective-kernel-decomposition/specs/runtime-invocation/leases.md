# Runtime invocation: admission and leases - Delta Spec

## ADDED Requirements

### Requirement: Privileged invocation uses one admission and lease path

Model tools, operator actions, trust-boundary calls, calls consuming caller authority, durable mutations, and host-effect-bearing internal calls must resolve a declared capability and current owner generation, combine admission policy, and receive a generation-bound execution lease before side effects.

An execution lease must identify call, principal, capability, owner generation, session/turn scope, admitted effects, issue generation, transition policy, and terminal closure. Dispatch must revalidate that the lease remains current. A lease closes or revokes exactly once and cannot be reused for another call.

#### Scenario: Unknown extension tool is invoked
Given an extension presents an invocation with no declared capability owner or effects
When the invocation pipeline evaluates the call
Then no execution lease is issued
And the call fails closed with typed owner or effect diagnostics

#### Scenario: Pure service query remains direct
Given an in-process service exposes a pure read-only query that consumes no caller authority or host effect
When another trusted in-process service calls it through a typed handle
Then the query need not use the privileged invocation pipeline
And it cannot use that direct path to obtain privileged effects

#### Scenario: Extension requests a nested host effect
Given an admitted extension or MCP tool returns a declarative HostAction
When the host evaluates the nested effect
Then it requires a live dispatching parent lease
And the nested effects must be contained by the parent's admitted effects
And the child dispatch identity can be consumed only once
And operator approval cannot replace missing parent, project, runtime, or origin authority

#### Scenario: Stale-generation lease reaches dispatch
Given a call received a lease for an owner generation that has since been revoked
When dispatch revalidates the lease
Then the owner is not invoked
And the lease closes exactly once with a stale-generation denial

#### Scenario: Lease close is repeated
Given an execution lease already has a terminal closure
When a delayed result or cleanup path attempts to close it again
Then no second terminal transition or audit settlement is recorded

### Requirement: Admission can only narrow authority

Kernel invariants, contribution eligibility, workspace policy, operator profile, task evidence, model constraints, RBAC, permissions, secret guards, sandbox requirements, and approval must combine monotonically. No downstream layer or adapter may widen an upstream denial.

#### Scenario: Model requests a denied capability
Given workspace policy denies a filesystem mutation capability
When the model requests admission or invokes its known name
Then no model or profile inference overrides the denial
And execution remains unavailable even if a stale schema still names the tool

### Requirement: Invocation state is crash-consistent

The runtime must persist `Prepared` before issuing authority, persist `Dispatched` before transport handoff, persist `Acknowledged` when the owner authoritatively accepts the call, and persist `Settled` before publishing ordinary completion. It must use a stable call and deduplication identity throughout. The execution lease must close or revoke exactly once after durable settlement or recovery classification.

#### Scenario: Runtime crashes after dispatch persistence
Given a mutating call has durable `Dispatched` state
And the runtime crashes before authoritative acknowledgement or settlement
When invocation state is recovered
Then the call is classified as unknown completion
And it is not reported as ordinary failure or success

#### Scenario: Runtime crashes before dispatch persistence
Given a call has durable `Prepared` state but no durable `Dispatched` state
When invocation state is recovered
Then recovery does not claim owner execution occurred
And any stale lease is revoked
And retry performs current admission again under a new lease while retaining the stable call identity where deduplication requires it

#### Scenario: Runtime crashes after acknowledgement
Given a mutating call has durable `Acknowledged` but no durable `Settled` state
When invocation state is recovered
Then the call is explicitly classified as `Unknown`
And ordinary completion and unqualified retry remain prohibited
Until an authoritative owner-status or deduplication protocol durably transitions it from `Unknown` to `Settled`

### Requirement: Unknown mutating completion is not blindly retried

A mutating invocation with unknown completion must not be retried unless the owner contract enforces idempotency or deduplication for the stable call identity.

#### Scenario: RPC disconnects after a mutating request
Given a mutating request was handed to an external owner
And transport disconnects before acknowledgement
And the owner declares no deduplication contract
When retry policy runs
Then the runtime records unknown completion and does not resend the mutation
And operator diagnostics identify the unresolved call identity

### Requirement: Settlement durability gates further mutation

Every mutating capability declaration must identify its durable mutation domain and fence key. If acknowledgement, terminal result, or audit settlement cannot be durably recorded after dispatch, the kernel must durably fence that declared domain before admitting another mutation against it, retain emergency recovery evidence containing call, owner, generation, lease, and fence identities, and withhold ordinary completion. The fence may be removed only by deterministic reconciliation or an explicit audited operator recovery decision.

#### Scenario: Durable writer fails after owner success
Given an owner reports a successful mutation
And the runtime cannot persist terminal settlement
When the invocation pipeline closes the attempt
Then it does not publish an ordinary completed outcome
And later mutations with the same durable mutation domain and fence key remain denied until recovery reconciles the call

#### Scenario: Revocation policy narrows during an active call
Given a destructive call has a lease whose transition policy requires immediate revocation
When authority narrows before settlement
Then the lease is revoked and bounded terminalization begins
And the call cannot inherit authority from the replacement generation

#### Scenario: Drain policy permits completion
Given a read-only call has a lease whose transition policy permits drain
When a replacement generation is promoted
Then the call may settle against its captured owner generation
And no new call is issued to the draining generation

### Requirement: Execution metadata replaces tool-name policy

Parallel safety, timeout class, retry class, idempotency, transaction behavior, required host effects, and transition policy must come from validated capability declarations rather than hard-coded invocation-name matching.

#### Scenario: New read-only tool supports parallel execution
Given a contribution declares a read-only, parallel-safe tool with bounded timeout and no privileged effects
When multiple independent calls are scheduled
Then the scheduler may execute them according to the declared concurrency policy
And no source-code name allowlist is required
