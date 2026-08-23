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

### Requirement: Registrations and candidate resources are generation-bound

Every Slice-2 registration and candidate-owned resource must identify one contribution generation. A session retains its admitted composition and preset generation; Slice 2 does not silently migrate an active or existing session and does not introduce a live migration event. Promotion must be atomic, and a graph-derived compatibility adapter must prevent legacy dispatch from reaching registrations excluded from the promoted graph. Enforceable generation-bound invocation leases, stale-call denial, and active-call drain or revocation belong to Slice 3. The selected loop may be replaced only at boot or a separately specified quiescent session boundary; the supervisor, admission combiner, and persistence protocol cannot be hot-replaced during an active turn.

#### Scenario: Legacy adapter follows the promoted graph
Given a validated candidate generation excludes a legacy registration
When that generation is promoted
Then the graph-derived EventBus adapter does not publish the excluded registration
And registration order cannot restore it

#### Scenario: Preset changes while a session is active
Given a session is bound to an admitted composition generation
When the underlying preset or profile is modified
Then the session's visible capabilities, provider policy, sandbox policy, and service participation remain unchanged
Unless a later slice defines and durably records a quiescent migration

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

### Requirement: In-process service implementations are generation-bound

Release-coupled in-process services must declare an `in_process_service` capability and pair it with exactly one typed implementation in the same candidate contribution generation. In-process services are not constitutional kernel services and do not gain third-party replacement authority. Graph validation, implementation parity, dependency activation, readiness, and publication of the typed service registry must be atomic. Published handles must identify capability, owner, and contribution generation and may be captured only at boot or a separately declared quiescent boundary. Candidate failure must preserve the complete prior graph and service registry. Service retirement must settle or honestly degrade every generation-owned resource before reporting a terminal cleanup state.

#### Scenario: Declared service has no implementation
Given a candidate declares an in-process service capability
And no typed implementation for that capability exists in the candidate generation
When candidate parity is validated
Then candidate publication fails
And the previous service registry remains callable

#### Scenario: Optional service is absent
Given a consumer declares an optional dependency on an in-process service
And no admitted implementation is available
When the candidate graph is activated
Then the consumer receives typed unavailable or degraded service state
And unrelated admitted capabilities remain callable
And diagnostics do not fabricate an active service

#### Scenario: Service generation changes
Given an active session captured a typed service handle at an admitted boundary
When a replacement service generation is promoted
Then the captured handle retains its original owner and generation identity
And the session does not silently switch implementations mid-turn

#### Scenario: No-resource service retires
Given an in-process service declares that it owns no tasks, subscriptions, temporary artifacts, or durable writers
When its generation retires
Then strict no-resource teardown settles deterministically
And retirement does not inherit a best-effort host-process cleanup claim

### Requirement: Behavior policy is a stateless optional service

The release-coupled behavior-policy implementation must publish `service:behavior-policy` / `interface:omegon-behavior-policy-v1` as an optional synchronous in-process service with strict no-resource teardown. Its object-safe contract may consume immutable host-normalized per-turn policy views and return only advisory unpinned task-mode inference, phase, drift, progress/evidence, pressure, pressure/meta-message, substantive-prose, and pathological-meta-response decisions. It must not retain conversation, tool, controller, frontend, or durable-session state. Session and host owners retain explicit operator mode parsing and correction recovery, declared tool capabilities, authoritative observation normalization, `IntentDocument`, persisted task mode, controller streaks, stuck/dead-mouse/meta counters, tool execution, event emission, and nudge insertion. Dynamic tool declarations and normalized observations are caller input rather than boot-cached service state.

#### Scenario: Behavior policy is present
Given an accepted behavior-policy generation was captured at boot
When each interactive, daemon/control, headless, bounded, Sentry, and ACP host evaluates a turn through its generation-tagged binding
Then results match canonical direct-policy fixtures BP01-BP09 pinned to commit `9c3a9860` and defined in the source design
And ACP transfers that binding across its worker boundary
And every host retains the binding's capability, owner, and generation identity
And the session retains all controller, recovery-counter, observation, and durable intent authority

#### Scenario: Behavior policy is absent
Given the active profile omits the optional behavior-policy service
When an ordinary text or tool turn executes
Then the turn remains callable with neutral advisory policy results
And every loop host and the ACP worker receives an absent optional binding plus graph unavailable or degraded evidence
And no service owner or generation identity is fabricated
And explicit operator mode declarations and existing session intent are preserved
And controller and recovery counters are held rather than advanced from synthetic no-progress
And no behavior-policy-derived first-turn, execution, evidence, or continuation nudge or meta retry is fabricated
And host-owned operator-correction recovery, completion reconciliation, plan reminders, stuck recovery, and text-only recovery remain unchanged
And the consumer performs no ambient registry lookup or direct classifier fallback

#### Scenario: Sessions share one behavior implementation
Given two sessions captured the same accepted behavior-policy generation
When their controller and recovery counters diverge
Then each session's decisions use only its own immutable input view
And the behavior service retains no cross-session state or generation-owned resource
