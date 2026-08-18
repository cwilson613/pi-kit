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

#### Scenario: Fallback route serves a request
Given the selected direct route is unavailable
And policy permits one compatible fallback
When the request is dispatched through that fallback
Then the route lease records selected and fallback identities plus the bounded reason
And later projections do not present the fallback as the originally selected route

### Requirement: One route authority serves every runtime host

Interactive, daemon, child-agent, and bounded execution must resolve provider routes through one typed route-service contract and record the same route-lease shape. Host adapters must not construct provider bridges or fallback chains independently.

#### Scenario: Daemon and interactive sessions select the same route policy
Given daemon and interactive sessions use the same profile, model intent, credentials, and provider health snapshot
When each resolves its next inference request
Then both use the same route authority and policy inputs
And any different result is attributable to recorded session or timing evidence rather than host-specific routing code

### Requirement: Fallback cannot broaden silently

Provider contributions and adapters must not infer arbitrary cross-family fallback. Fallback compatibility must be declared and narrowed by current route policy and admission.

#### Scenario: Undeclared model-family substitution is proposed
Given a provider candidate can technically accept an OpenAI-compatible request
But no fallback compatibility relation exists for the selected model family
When route resolution runs
Then the candidate is not selected as fallback

### Requirement: Driver replacement is quiescent

The selected loop driver and provider route service may be replaced only at boot or a durably recorded quiescent session migration boundary, never during an active turn.

#### Scenario: Replacement is requested mid-turn
Given a session has an active turn
When configuration requests replacement of its loop driver or route service
Then the replacement remains pending or is rejected
And the active turn retains its captured driver and route generations

### Requirement: Loop policy depends only on typed runtime contracts

The release-coupled loop driver must depend on typed session-transition, route-lease, context-assembly, and privileged-invocation contracts rather than concrete provider, tool, memory, lifecycle, or frontend implementations.

#### Scenario: Optional lifecycle service is absent
Given a product profile omits lifecycle services
When the loop executes a turn that does not require lifecycle capability
Then the loop operates through its typed contracts without importing or branching on a lifecycle implementation name
