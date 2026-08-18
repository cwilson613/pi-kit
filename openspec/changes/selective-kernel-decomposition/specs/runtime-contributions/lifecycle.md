# Runtime contributions: lifecycle and generations - Delta Spec

## ADDED Requirements

### Requirement: Contribution declarations govern activation

Runtime contributions must publish stable identity, owner tier, invocation bindings, dependencies, conflicts, lifecycle policy, platform requirements, effects, protocol range, timeout class, retry class, idempotency/deduplication semantics, transition policy, and surface support before ordinary activation. The resulting graph, not registration order, governs ownership and activation.

#### Scenario: Two contributions claim one invocation
Given two candidate declarations claim the same invocation without an explicit replacement relation
When the candidate graph is validated
Then validation fails with both owners and the ambiguous invocation
And neither candidate graph nor first registration is promoted

#### Scenario: Dependency cycle exists
Given candidate contributions contain a dependency cycle
When graph validation runs
Then every cycle member is reported deterministically
And no contribution in the invalid candidate generation is ordinarily activated

#### Scenario: Required owner or protocol support is missing
Given a candidate depends on a missing owner or unsupported protocol range
When graph validation runs
Then the candidate graph is rejected with the unsatisfied requirement
And no subset of its registrations is promoted

#### Scenario: Observed effect was not declared
Given a quarantined contribution requests a host effect absent from its frozen declaration
When the request is evaluated
Then the request is denied
And the contribution is failed or quarantined according to policy

### Requirement: Trust admission precedes dynamic code execution

Before executing contribution-controlled code, the kernel must validate a non-executable static preflight manifest containing stable identity, supported protocol range, minimum dependencies, requested trust class, requested confinement boundary, and probe requirements. Manifest declarations only request trust or confinement and cannot grant either. Trusted-code admission must originate from kernel-controlled release or operator policy. Verified confinement means a host-established OCI or OS boundary that prevents direct filesystem, process, secret, and network access and forces privileged effects through kernel brokers. Capability admission occurs only after an admitted probe negotiates and freezes its declarations in quarantine. An unsandboxed admitted process remains trusted host-authority code even while quarantined.

#### Scenario: Unsandboxed MCP server requests probing
Given an MCP server command would run with operator host authority
And the operator has not admitted it as trusted code
And verified confinement is unavailable
When contribution discovery reaches the probe stage
Then the process is not spawned
And diagnostics report that trust admission is required

#### Scenario: Confinement was requested but cannot be established
Given a contribution requires a specific OCI or OS confinement boundary
When the host cannot establish that boundary
Then admission fails closed
And the runtime does not silently launch an unconfined process

### Requirement: Dynamic negotiation is quarantined and bounded

An admitted dynamic probe must run without brokered host-effect leases, freeze its negotiated declaration set, satisfy a readiness deadline, and pass graph validation before atomic promotion.

#### Scenario: Probe never becomes ready
Given a quarantined contribution has started
When its readiness deadline expires
Then the candidate is marked failed or quarantined
And its complete host-owned resource tree is settled within the applicable boundary
And the previous active generation remains callable

### Requirement: Candidate activation is rollback-covered before publication

Candidate graph construction, dependency activation, registration, readiness, and promotion must remain unpublished until the candidate is complete. Failure at any candidate stage must leave the previous generation callable, publish none of the candidate's registrations or authority, and settle every candidate-owned resource within the host ownership boundary.

#### Scenario: Candidate fails after partial registration
Given a candidate has created registrations and resources but has not been promoted
When dependency activation or post-readiness initialization fails
Then candidate registrations remain invisible to model and operator projections
And candidate-owned resources are settled or honestly reported degraded across an unowned boundary
And strict-cleanup profiles refuse promotion when cleanup cannot be verified

#### Scenario: Required composition remains unresolved
Given a product profile requires a contribution that cannot become ready
When the readiness or retry budget is exhausted
Then that product runtime is not published
And recovery diagnostics and the maintenance executable remain available

#### Scenario: Contribution enters a crash loop
Given a contribution repeatedly exits during bounded restart attempts
When its crash/backoff policy is exhausted
Then it enters failed or quarantined state
And restart attempts stop until an authorized recovery decision or changed generation

### Requirement: Contribution health and retirement are typed

Heartbeat loss, dependency degradation, bounded restart/backoff, drain, retirement, quarantine, and forced cleanup must produce typed state with owner, generation, last completed boundary, and bounded reason. Health changes can narrow callability but cannot silently select a replacement owner.

#### Scenario: Required dependency becomes degraded
Given an active contribution loses a required dependency
When dependency health is recomputed
Then new dependent calls are denied or degraded according to declared policy
And the contribution does not continue under stale dependency authority

#### Scenario: Heartbeat is lost
Given an external contribution requires heartbeat evidence
When the heartbeat budget expires
Then its generation enters the declared degraded, draining, or failed state
And diagnostics record the last observed heartbeat and lifecycle boundary

#### Scenario: Drain deadline expires
Given an old generation is draining active calls
When its declared drain deadline expires
Then host-owned calls and resources enter bounded revocation and cleanup
And unowned cross-boundary resources are reported unverified rather than falsely retired

### Requirement: Registrations and calls are generation-bound

Every registration and invocation must identify one contribution generation. A session must retain its admitted composition and preset generation until a declared, durably recorded quiescent migration boundary. Promotion must be atomic; old generations drain or revoke under declared transition policy and cannot receive new calls after replacement. The selected loop may be replaced only at boot or a quiescent session boundary; the supervisor, admission combiner, and persistence protocol cannot be hot-replaced during an active turn.

#### Scenario: Replacement occurs during an active read-only call
Given an old generation owns an active call whose transition policy permits drain
When a validated replacement generation is promoted
Then new calls resolve to the replacement
And the old call completes against its captured generation before retirement

#### Scenario: Authority narrows during an active destructive call
Given an active call depends on authority that is revoked immediately by policy
When the new generation is promoted
Then the old call receives revocation and bounded terminalization
And it cannot inherit the new generation's authority

#### Scenario: Preset changes while a session is active
Given a session is bound to an admitted composition generation
When the underlying preset or profile is modified
Then the session's visible capabilities, provider policy, sandbox policy, and service participation remain unchanged
Until a quiescent migration is durably admitted and recorded

### Requirement: Resource ownership and cleanup claims are honest

Every process, task, socket, listener, subscription, temporary file, and durable writer must have one recorded owner and generation. For every resource within Omegon's ownership boundary, cancellation, timeout, failed startup, generation replacement, and shutdown must settle the complete owned tree before the corresponding terminal state is reported. Cross-boundary cleanup is represented as degraded or unverified rather than settled.

#### Scenario: Strict profile encounters a cross-boundary process
Given a profile requires strict process-tree cleanup
And a candidate transport crosses into a lifecycle boundary Omegon cannot own
When contribution eligibility is evaluated
Then the transport is rejected for that profile
And diagnostics identify the unsupported cleanup guarantee

### Requirement: Optional contributions degrade locally

Failure, absence, corruption, or quarantine of any optional contribution must not prevent the constitutional recovery substrate or maintenance executable from reaching diagnostic-ready state. The maintenance profile cannot mark project configuration, project contributions, mutable packs, MCP servers, memory, lifecycle, orchestration, the normal TUI, or the default loop as startup requirements. Other product profiles may mark a contribution required; failure then prevents publication of that product runtime but not recovery diagnostics or maintenance startup.

#### Scenario: Optional lifecycle service fails activation
Given the active profile does not require lifecycle capability
When the lifecycle contribution fails activation
Then the runtime reports the capability degraded or unavailable
And unrelated admitted capabilities remain callable
