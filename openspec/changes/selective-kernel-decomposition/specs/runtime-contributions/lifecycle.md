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
Then the candidate is marked failed or quarantined only if its complete host-owned resource tree settles
And otherwise its rejected resources remain under a nonterminal degraded owner
And the previous active generation remains callable

### Requirement: Candidate activation is rollback-covered before publication

Candidate graph construction, dependency activation, registration, readiness, and promotion must remain unpublished until the candidate is complete. Failure at any candidate stage must leave the previous generation callable and publish none of the candidate's registrations or authority. Every candidate-owned resource within the host ownership boundary must settle before cleanup-settled, terminal-failure, or ownership-release claims; a deadline may return candidate rejection while unresolved resources remain under a nonterminal degraded owner for later retry.

#### Scenario: Candidate fails after partial registration
Given a candidate has created registrations and resources but has not been promoted
When dependency activation or post-readiness initialization fails
Then candidate registrations remain invisible to model and operator projections
And candidate resources are settled, retained as nonterminal degraded within an owned boundary, or reported unverified across an unowned boundary
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

### Requirement: Managed in-process services drain and clean up by generation

Resource-bearing in-process services must use a managed contract distinct from no-resource read services. A managed implementation must not escape as a raw `Arc` or borrowed consumer reference. An object-safe request/response/error contract executes only inside a generation-owned call task through an identity-bearing handle; the handle owns admission, accounting, cancellation, panic handling, and typed draining/degraded/retired errors. Every handle consults one shared admission-table snapshot keyed by contribution generation, and one table replacement is the publication linearization point for all unchanged, replaced, removed, and new managed generations. Candidate resources must register under exactly one contribution generation before readiness and remain unpublished until promotion. Candidate rollback and active-generation retirement must use the same bounded cleanup engine. Unchanged contribution generations transfer without cleanup. Strict cleanup requires positive settlement before cleanup-settled, retired, terminal-failure, or ownership-release claims. A strict failure is nonterminal degraded cleanup with retained ownership and bounded evidence of identity and attempted stop/force-stop; best-effort cross-boundary cleanup may be unverified.

#### Scenario: Candidate managed service is rejected
Given a candidate owns managed resources that are not yet published
When graph, implementation, readiness, resource, or policy validation fails
Then only candidate admission and resources are closed
And each resource is either settled or retained under a nonterminal degraded or unverified owner
And the prior graph, typed registry, handles, and resources remain callable
And the cleanup deadline is not restarted for each resource

#### Scenario: Managed service generation is replaced
Given an admitted call holds the old managed generation
When an authorized candidate is atomically promoted
Then the old gate stops admitting new calls before the new generation is observable
And the admitted call may complete only within its declared active-call deadline
And deadline expiry cancels, aborts, and joins remaining generation-owned call tasks before resource cleanup
And a stale handle returns a typed draining, degraded, or retired error without switching implementations

#### Scenario: Managed cleanup degrades after publication
Given a replacement generation has already been published
When a strict old-generation resource cannot settle by the cleanup deadline
Then publication remains successful with nonterminal degraded cleanup evidence
And the old owner remains available for later cleanup retry or shutdown
And its lifecycle is not reported retired or cleanup-settled

#### Scenario: Cross-boundary cleanup cannot be verified
Given a best-effort resource crosses an ownership boundary Omegon cannot settle
When retirement exhausts its cleanup deadline
Then cleanup is unverified rather than settled
And diagnostics retain the resource owner, generation, boundary, and bounded reason

#### Scenario: Boot-only service changes after publication
Given an accepted composition already exists
When a candidate changes or introduces a boot-only in-process service
Then publication is rejected before old-generation admission closes

#### Scenario: Quiescent service changes after publication
Given an accepted composition already exists
And a candidate changes a service declared for quiescent-session activation
When replacement is requested
Then publication requires a current runtime/session-bound one-use supervisor proof
And stale, cross-session, replayed, active-turn, unresolved-invocation, or active-call evidence fails closed

#### Scenario: Cleanup follows declared dependencies
Given managed resource controllers declare an acyclic cleanup dependency graph
When generation cleanup begins
Then every controller receives an idempotent stop request and a force-stop request only when cooperative settlement fails or no await budget remains
And stop, conditional force-stop, and settlement follow reverse topological order under one non-resetting cleanup deadline
And missing dependencies or cycles prevent candidate publication

#### Scenario: Publication crosses its linearization point
Given all candidate preparation and validation has succeeded
When the shared admission table is replaced with one complete accepting/draining generation map
Then publication is irrevocably committed and the candidate graph, registry, and generation become visible before exclusive mutation returns
And failure before that point returns rejected with candidate rollback evidence
And cleanup degradation after that point returns published with retirement evidence rather than a rollback error

#### Scenario: Runtime shuts down with managed services
Given the active composition owns managed resources
When a normal host shutdown begins
Then managed call admission closes before asynchronous resource settlement
And settlement completes before process-level runtime ownership is removed
And degraded shutdown leaves final ownership evidence for maintenance or stale pruning
And Drop alone cannot claim asynchronous cleanup success

#### Scenario: Publication caller is cancelled after commit
Given managed publication crossed its linearization point and owns old-generation cleanup
When the requesting future is cancelled or shutdown begins concurrently
Then EventBus retains and joins the cleanup task through one serialized lifecycle owner
And no resource controller or call task is detached

### Requirement: Codescan is one managed workspace index service

The optional release-coupled codescan contribution must publish `service:codescan` / `interface:omegon-codescan-v1` as a boot-captured managed service owned by `feature:codescan`. One serial generation-owned worker must exclusively own the workspace SQLite connection, indexing, HEAD freshness checks, and BM25 construction for `codebase_search`, `codebase_index`, and `request_context(kind="code")`. The concrete implementation, worker, connection, `ScanCache`, and `Indexer` must not escape through a consumer API. The worker must be a strict `Task` resource that depends on a strict `DurableWriter` resource. Cleanup must stop and join the worker before it claims that the SQLite connection closed. Codescan owns no subprocess or cross-boundary resource.

#### Scenario: Tool and context requests share one writer
Given an accepted codescan generation was captured at boot
When tool search, explicit indexing, and code-context requests execute concurrently
Then every request uses the same generation-tagged managed handle and serial worker
And only that worker opens or mutates the workspace codescan database
And consumer adapters perform no ambient lookup or direct cache fallback

#### Scenario: Incremental path update is cancelled
Given the active index contains a previously committed path
When cancellation occurs after replacement preparation but before that path transaction commits
Then the transaction rolls back the complete path replacement
And the previously committed path remains searchable without partial replacement rows
And pruning and HEAD metadata do not advance for the incomplete run

#### Scenario: Full invalidation is cancelled
Given the active index contains a searchable committed generation
When `codebase_index` with `invalidate=true` is cancelled before rebuild commit
Then the complete rebuild transaction rolls back
And the prior searchable index, file state, pruning state, and HEAD metadata remain active

#### Scenario: Codescan shuts down
Given the active codescan generation owns its worker and SQLite writer
When normal host shutdown closes managed admission
Then admitted calls settle or are cancelled within the active-call deadline
And the worker stops and joins before the SQLite connection reports settlement
And no HEAD check, index command, connection, or cache owner survives retirement

#### Scenario: Codescan handle becomes stale
Given a consumer retained a codescan handle from an admitted generation
When that generation becomes draining, degraded, or retired
Then the handle returns the corresponding typed managed-service error
And the consumer does not open the database or switch to another implementation

#### Scenario: Codescan is absent
Given the optional codescan service is unavailable at boot
When a codescan tool or mixed context request executes
Then the codescan tool remains declared and returns typed unavailable evidence
And the code context part reports unavailable rather than no matches
And unrelated requested context kinds remain callable
And no service owner, generation, SQLite connection, or direct indexing fallback is fabricated

### Requirement: Lifecycle and OpenSpec share one managed repository owner

The optional release-coupled lifecycle contribution must publish `service:lifecycle` / `interface:omegon-lifecycle-v1` as a boot-captured managed service owned by `feature:lifecycle`. One serial generation-owned worker must own the loaded opsx FSM and ledger, design/OpenSpec artifact coordination, repository revision, reconciliation, transaction recovery, and every Omegon-authored lifecycle mutation. Git-native design and OpenSpec artifacts remain canonical semantic content; the ledger remains enforcement and audit state. Consumers must retain only the managed handle or owned response DTOs and must not receive the implementation, `Lifecycle`, `JsonFileStore`, `OpenSpecRepository`, design repository/provider locks, or a direct filesystem fallback. The worker must be a strict `Task` resource that depends on a strict `DurableWriter` resource and must stop and join before writer settlement.

#### Scenario: Lifecycle readers share one repository revision
Given an accepted lifecycle generation was captured at boot
When tools, context, ACP, TUI, Web, IPC, workflow, work aggregation, and repository projections read lifecycle state
Then each read uses the same generation-tagged managed handle or immutable output from that handle
And responses distinguish absent, malformed, unreadable, stale, drifted, and recovery-required state
And no consumer independently loads the ledger or scans canonical lifecycle paths as authority

#### Scenario: External authoring changes canonical content
Given a client observed lifecycle repository revision N
And an external author edits canonical design, task, or spec Markdown
When the client requests a managed mutation with expected revision N
Then the service detects the changed artifact identity and rejects the stale revision before mutation
And it parses, health-checks, and explicitly reconciles the external content before a later mutation can commit
And it does not overwrite the external edit from a stale cached projection

#### Scenario: Lifecycle mutation spans artifacts and ledger
Given a design or OpenSpec operation changes more than one canonical artifact or ledger record
When the managed service commits that operation
Then it validates the complete operation against one current repository revision
And it records a versioned, checksummed, repository-relative, path-contained transaction before publishing partial resources
And restart recovery deterministically produces the complete pre-operation or post-operation state
And success is not returned for a partial durable prefix

#### Scenario: Lifecycle persistence fails
Given a validated lifecycle operation has staged an in-memory FSM change
When temporary write, file durability, rename, parent-directory durability, or ledger persistence fails
Then the call returns a typed persistence or recovery-required error
And a failed pre-commit operation does not remain in memory for a later save to publish accidentally
And any ambiguous post-commit state remains owned and journaled for deterministic recovery

#### Scenario: Lifecycle operation is replayed
Given a lifecycle mutation committed with a stable operation identity
When a client repeats that identity after an ambiguous response or restart
Then the service returns the recorded committed outcome and revision
And it does not append another audit entry or apply the mutation twice

#### Scenario: Lifecycle archive journal is damaged or hostile
Given startup discovers a malformed journal or a journal with an unsupported version, wrong repository identity, invalid phase, content mismatch, or path outside the selected repository roots
When lifecycle recovery runs
Then the service does not follow or mutate the untrusted path
And it reports typed recovery-required or quarantined evidence with bounded detail
And unrelated readable lifecycle state remains diagnosable without claiming the damaged operation settled

#### Scenario: Lifecycle shuts down
Given the active lifecycle generation owns its repository worker and durable writer
When normal host shutdown closes managed admission
Then admitted calls settle or are cancelled within the active-call deadline
And the worker stops and joins after all queued and active mutations cease
And transaction, artifact, and ledger writes settle before the durable writer reports settlement
And no worker, loaded mutable ledger, temporary write, or unresolved unrecorded mutation survives retirement

#### Scenario: Lifecycle is absent
Given the optional lifecycle service is unavailable at boot
When a lifecycle tool, semantic projection, or mixed context request executes
Then lifecycle tools and surfaces remain declared with typed unavailable evidence
And lifecycle-derived context and work data are omitted without blocking unrelated requested kinds
And no service owner, generation, ledger, scanner, artifact repository, or direct filesystem fallback is fabricated

#### Scenario: Session focus remains outside repository authority
Given two sessions share one accepted lifecycle service generation
When they focus different design nodes or advance independent context TTLs
Then each session retains its own focus and context state
And neither focus operation changes the lifecycle repository revision or durable ledger

#### Scenario: OpenSpec roots conflict
Given both the primary and legacy OpenSpec roots contain lifecycle artifacts
When the lifecycle candidate resolves repository resources
Then readiness fails with a typed conflicting-authority diagnostic
And the service does not merge, shadow, or mutate either root

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
