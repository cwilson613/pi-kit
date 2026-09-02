# kernel-composition/artifact-profiles - Delta Spec

## ADDED Requirements

### Requirement: Reduced kernel executes bounded agent turns

The reduced kernel artifact must execute bounded agent turns without importing
product-domain implementations or establishing host-specific route, session, or
invocation authority. Deterministic conformance execution must remain available
without network credentials, while production execution must use the same typed
runtime authorities and terminal outcome contract as full-product bounded work.

#### Scenario: Scripted kernel turn proves the loop boundary
Given the reduced kernel has no provider credentials or optional domain artifacts
When its deterministic conformance turn executes
Then exactly one scripted model request reaches one terminal completion within explicit event and time bounds
And the result identifies scripted conformance provenance without claiming an admitted production route

#### Scenario: Provider-backed kernel turn completes
Given the reduced kernel has an admitted provider route and a bounded task
When the operator invokes its bounded execution entrypoint
Then route selection, request execution, and terminal settlement use shared typed runtime authorities
And the structured result and exit status match the bounded task contract without loading product domains

#### Scenario: Kernel turn exhausts a bound
Given a reduced-kernel turn reaches its admitted time, turn, token, event, or tool bound
When the next governed action would exceed that bound
Then execution stops before that action with a typed exhausted outcome
And provider, invocation, and child-process authority settle before exit

### Requirement: First-party domains have explicit packaging classes

Every first-party runtime domain must appear exactly once in a checked inventory
that records its canonical owner, packaging class, runtime boundary, extraction
disposition, and composition evidence. A domain may be constitutional resident,
host service, signed core component, shipped content, or operator-managed SDK
extension; runtime disablement does not rewrite that packaging class.

#### Scenario: First-party domain inventory is checked
Given the workspace adds or retains a first-party runtime domain
When composition policy validation runs
Then the domain has exactly one packaging class, canonical owner, and extraction disposition
And an unknown, duplicate, or contradictory classification fails the gate

#### Scenario: Managed service remains in the host
Given a managed service has authority, cost, or failure-boundary evidence that does not justify extraction
When its classification is reviewed
Then it remains an explicit host service with typed lifecycle ownership
And the inventory does not falsely present it as a signed core component

#### Scenario: Domain graduates to a core component
Given a first-party domain is classified as a signed `core:*` component
When its promotion is validated
Then portable contracts, signed identity, kernel absence, additive restoration, full-product retention, cleanup, and aggregate budget evidence are present
And SDK extension metadata cannot grant that product-component authority

#### Scenario: Core qualification evidence is incomplete
Given a signed `core:*` qualification record omits, duplicates, or aliases a required executor
When composition policy validation runs
Then promotion fails before package or runtime publication
And evidence from another component or an SDK extension cannot satisfy the missing boundary

#### Scenario: Core qualification evidence is complete
Given a component has portable contracts and a signed release identity
And its qualification record names kernel-absence, additive-restoration, full-product, policy, protocol, cleanup, budget, inventory, and non-promotion executors
When the generic component promotion gate runs
Then every executor passes for the same component and wire identity
And the component becomes eligible for signed-core promotion without a component-specific policy exception
