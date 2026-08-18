# Runtime capabilities: authoritative declarations - Delta Spec

## MODIFIED Requirements

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

### Requirement: The pre-migration declaration inventory remains authority-neutral

The read-only declaration inventory must remain authority-neutral through Slice 1 and until an approved Slice-2 authoritative graph generation passes graph validation, admission, generation-lease, and projection/dispatch parity gates. After those gates pass, construction, projection, and privileged dispatch may migrate to that graph and legacy EventBus authority may be removed. Authority-neutral compatibility mode must remain explicit rather than allowing both paths to execute independently.

#### Scenario: Authoritative graph is not ready
Given declarations have been collected
And generation-bound dispatch parity has not passed
When the runtime starts under a compatibility profile
Then legacy execution authority remains active
And diagnostics distinguish legacy-owned execution from candidate graph state

#### Scenario: Authority migration completes
Given authoritative graph, admission, lease, and parity gates pass
When the new generation is promoted
Then schema projection and privileged dispatch consume the same graph generation
And the legacy path cannot execute a capability denied by that graph
