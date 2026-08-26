# runtime-capabilities/declarations - Baseline

### Requirement: Runtime contributions have stable declarations

Every candidate runtime contribution must publish renderer-neutral capability declarations before ordinary activation. Declarations must include stable identity, capability kind, owner and trust tier, invocation bindings, dependencies, conflicts, lifecycle and transition policy, platform requirements, effects, protocol range, timeout class, retry class, idempotency/deduplication semantics, and surface support.

#### Scenario: Existing tool declaration is adapted
Given a legacy feature owns a registered model tool
When its candidate declaration is constructed
Then the stable tool ID and canonical invocation name remain unchanged
And the adapter supplies explicit owner, effect, lifecycle, and execution metadata before activation

#### Scenario: Command alias is preserved
Given multiple aliases resolve to one canonical operator action
When declarations are constructed
Then the aliases remain invocation bindings to one capability identity
And no alias creates independent execution authority

### Requirement: Registry integrity is validated before authority migration

Candidate graph validation must reject duplicate capability ownership without a valid explicit replacement relation, ambiguous invocation names, dependency cycles, missing required owners or services, unsupported protocol ranges, incompatible platform requirements, dangling groups or aliases, and requested or observed effects absent from the frozen declaration. Diagnostics must identify all conflicting owners and dependencies deterministically.

#### Scenario: Invalid candidate graph is evaluated
Given a candidate graph contains duplicate ownership and a missing required service
When validation runs
Then diagnostics report both defects in deterministic order
And no subset of the candidate declarations is promoted or made callable

### Requirement: Slice one remains authority-neutral

Declaration inventory construction must not alter existing tool filtering or execution behavior.

#### Scenario: Callable inventory remains unchanged
Given an existing operator profile and registered tool set
When the declaration inventory is built and validated
Then the legacy callable tool names are unchanged
And tool dispatch continues through the existing EventBus authority path

### Requirement: Slice-2 composition authority retains a legacy dispatch adapter

The read-only declaration inventory must remain authority-neutral through Slice 1 and until an approved Slice-2 graph generation passes graph validation, admission, activation, readiness, and projection parity gates. After those gates pass, the graph becomes authoritative for composition, activation, and projection. Slice 2 must derive a one-way legacy EventBus registration adapter from the promoted graph; the EventBus cannot independently select or reactivate an owner rejected by that graph. Generation-bound privileged invocation leases and dispatch migration remain Slice 3. Compatibility mode must remain explicit rather than allowing both paths to select owners independently.

#### Scenario: Authoritative graph is not ready
Given declarations have been collected
And graph-derived legacy registration parity has not passed
When the runtime starts under a compatibility profile
Then legacy execution authority remains active
And diagnostics distinguish legacy-owned execution from candidate graph state

#### Scenario: Composition authority migration completes
Given authoritative graph, admission, activation, readiness, and parity gates pass
When the new generation is promoted
Then schema projection and legacy compatibility registrations derive from the same graph generation
And the legacy path cannot execute a capability denied by that graph
And privileged invocation authority remains assigned to Slice 3
