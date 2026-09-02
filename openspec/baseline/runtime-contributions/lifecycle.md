# runtime-contributions/lifecycle - Baseline

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

### Requirement: Dynamic transports share one contribution lifecycle

Native extensions, MCP process and HTTP servers, and executable manifest contributions must enter one transport-neutral discovered-candidate inventory. Discovery must capture stable identity, source kind, source digest, protocol range, requested trust and confinement, and probe requirements without evaluating a manifest, spawning a process or container, connecting to a service, resolving secrets, or publishing registrations. After trust admission, the transport adapter may probe within its existing confinement boundary. The shared lifecycle must bound readiness, freeze declarations, stage the graph, publish atomically, retain restart and quarantine evidence, and own rollback, replacement, stale-generation denial, and resource cleanup. Transport adapters retain protocol framing, HostAction checks, MCP resources and prompts, widgets, secret delivery, process-tree ownership, and remote cleanup limitations.

#### Scenario: Discovery inventories every supported transport without execution
Given native extension, MCP process, MCP HTTP, manifest script, manifest HTTP, and supported manifest OCI configuration is present
When dynamic contribution discovery builds the candidate inventory
Then every candidate has stable identity, source kind, and source digest evidence
And no candidate process, container, connection, manifest evaluator, secret lookup, or registration has started

#### Scenario: Trust admission rejects a discovered candidate
Given a discovered dynamic candidate is absent from trusted-code policy and has no verified confinement admission
When the shared lifecycle evaluates trust admission
Then the candidate is rejected before its transport adapter probes
And unrelated admitted and optional-absence candidates continue through the lifecycle

#### Scenario: A dynamic probe times out
Given an admitted candidate has started a transport-specific probe
When the shared readiness deadline expires before declarations freeze
Then none of its tools, resources, prompts, widgets, or other registrations are published
And its complete generation-owned resource tree is settled or retained with degraded or unverified cleanup evidence

#### Scenario: Graph publication rejects a prepared generation
Given an admitted candidate froze declarations and created generation-owned resources
When graph staging or publication rejects the candidate generation
Then the prior published generation remains callable
And the shared lifecycle rolls back only candidate resources without transport-specific manual cleanup authority

#### Scenario: Dynamic generation becomes stale
Given a dynamic contribution generation was replaced or removed
When a handle or restart attempt from that generation reaches its transport adapter
Then the shared lifecycle denies it as draining, degraded, quarantined, or retired
And it cannot publish registrations or transfer resources into the current generation

#### Scenario: Dynamic generation shuts down
Given one published generation owns native, MCP, or executable manifest resources
When normal shutdown closes dynamic contribution admission
Then cleanup runs once through the shared generation owner
And process-backed resources are terminated and joined within their transport boundary
And remote HTTP cleanup remains honestly unverified when the host cannot prove peer settlement

#### Scenario: Optional dynamic contribution is absent
Given no candidate exists for an optional native, MCP, or manifest contribution
When the dynamic lifecycle publishes the runtime composition
Then unrelated contributions remain callable
And no lifecycle owner, registration, transport connection, or generation identity is fabricated for the absent contribution

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

The optional release-coupled codescan contribution must run as a supervised native extension and expose a versioned codescan RPC interface to boot-captured host adapters. One serial extension-owned worker must exclusively own the workspace SQLite connection, indexing, HEAD freshness checks, and BM25 construction for `codebase_search`, `codebase_index`, and `request_context(kind="code")`. The concrete engine, worker, connection, `ScanCache`, and `Indexer` must not escape through the wire contract. The extension process must stop accepting work, cancel active and queued requests, join its worker, and close SQLite before graceful shutdown completes. The host extension supervisor must terminate and reap the complete process group when graceful shutdown fails.

#### Scenario: Tool and context requests share one extension worker
Given a compatible codescan extension was admitted at boot
When tool search, explicit indexing, and code-context requests execute concurrently
Then every request uses the captured extension RPC handle and one serial worker
And only that extension worker opens or mutates the workspace codescan database
And host adapters perform no ambient lookup or direct cache fallback

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
Given the active codescan extension owns its worker and SQLite writer
When normal host shutdown closes extension admission
Then admitted calls settle or are cancelled within the active-call deadline
And the extension joins its worker and closes SQLite before graceful completion
And a non-cooperative process tree is terminated and reaped by the host supervisor

#### Scenario: Codescan handle becomes unavailable
Given a consumer retained the boot-captured codescan RPC binding
When the extension is quarantined, stopped, incompatible, or retired
Then the binding returns typed unavailable evidence
And the consumer does not open the database or switch to another implementation

#### Scenario: Codescan is absent
Given the codescan extension is unavailable at boot
When a codescan tool or mixed context request executes
Then the host-owned codescan tool remains declared and returns typed unavailable evidence
And the code context part reports unavailable rather than no matches
And unrelated requested context kinds remain callable
And no extension process, SQLite connection, or direct indexing fallback is fabricated

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

### Requirement: Durable memory is an optional managed service

The release-coupled memory implementation must publish `service:memory` / `interface:omegon-memory-v1` as an optional boot-only managed service. One serial worker must own the selected project store, optional global store, SQLite and WAL state, durable mind-scoped facts, edges, episodes, vectors, JSONL synchronization, and configured Codex-vault synchronization. Consumers must use one boot-captured generation-tagged handle or immutable service output. They must not retain a backend, connection, vault writer, implementation callback, or direct persistence fallback. The version-1 contract does not require a standalone durable mind-record or parent-mutation API.

#### Scenario: Memory is present
Given an accepted memory generation was captured at boot
When tools, context, lifecycle ingestion, session-end persistence, embedding-result writes, or status surfaces need durable memory
Then they use the captured managed handle and return existing compatible DTOs or typed service errors
And one service worker owns all project and optional global durable store access
And no consumer opens SQLite, imports or exports JSONL, or synchronizes the vault directly

#### Scenario: Independent mutations remain concurrent in meaning
Given two accepted requests store independent facts against the same service generation
When the serial worker commits both requests
Then each request uses stable operation identity and existing content-addressed idempotency
And neither request fails because an unrelated fact changed a global store revision
And replay of either committed operation does not duplicate its durable effect

#### Scenario: Targeted and imported mutations preserve conflict policy
Given a request targets an existing fact or imports a JSONL record
When the request conflicts with a newer durable entity version
Then the targeted mutation returns a typed precondition conflict or the JSONL import applies the existing Lamport conflict rule
And all multi-record SQLite effects remain atomic
And deterministic tie-breaking produces the same result after reopen

#### Scenario: Embeddings are unavailable
Given the managed memory service is present but no embedding provider can produce a vector
When a consumer recalls facts
Then retrieval uses the existing deterministic non-vector path
And durable fact, episode, graph, JSONL, and vault operations remain callable
And the service does not fabricate an embedding model or retain provider credentials

#### Scenario: Session and provider state remain outside memory
Given two sessions share one accepted memory service generation
When they select different minds, pin different working facts, advance different context TTLs, or run different extraction providers
Then each session retains its own selection, pins, context state, and provider tasks
And durable mind-scoped records and submitted persistence results remain service-owned
And provider tasks can persist only through the captured handle and must settle before managed shutdown

#### Scenario: Memory resources shut down
Given the memory worker is `resource:memory-worker` and depends on `resource:memory-writer`
When managed shutdown closes admission
Then active calls receive one 30-second drain deadline
And cleanup receives one non-resetting 5-second deadline
And the queue and worker stop and join before SQLite, WAL, JSONL, and vault writer settlement
And timeout or failure retains degraded ownership without claiming retirement

#### Scenario: Memory is absent
Given the optional memory service is unavailable at boot
When a memory tool, status surface, or mixed context request executes
Then memory tools and surfaces remain declared with typed unavailable evidence
And durable memory context is omitted while unrelated context and host-owned compaction continue
And no project or global store, JSONL synchronizer, vault writer, service owner, or generation is fabricated

#### Scenario: Offline maintenance respects service ownership
Given a stopped-runtime schema or selected-root migration or a one-shot embedding backfill is requested
When the operation runs
Then migration remains a non-concurrent maintenance owner
And embedding backfill uses a bounded managed memory composition that shuts down before exit
And neither path can bypass an active memory generation

### Requirement: Context compaction planning is an optional managed service

The release-coupled context/compaction implementation must publish `service:context-compaction` / `interface:omegon-context-compaction-v1` as an optional boot-only managed service. The service must select compaction eligibility, keep windows, evicted-entry counts, and provider payloads only from immutable host-normalized conversation entries. Consumers must use one boot-captured generation-tagged handle and must not perform ambient registry lookup or call a direct compaction planner after cutover.

`ContextManager`, prompt and injection state, canonical conversation mutation, semantic compaction facts, supervisor admission, provider route selection and dispatch, context metrics, and frontend events must remain session or host owned. The service must not receive a session authority handle, provider bridge, mutable conversation, frontend sender, or durable writer.

#### Scenario: Context compaction service is present
Given an accepted context/compaction generation was captured at boot
When automatic pressure, provider overflow, a feature request, or a manual command asks for a compaction plan
Then the consumer invokes the same exact-generation handle with immutable host-normalized conversation entries
And the returned eligibility, keep window, evicted-entry count, reason, and provider payload preserve existing compaction behavior
And the host alone admits semantic compaction, dispatches the provider, applies or repairs conversation state, and publishes metrics and events

#### Scenario: Context compaction service is absent
Given the optional context/compaction service is unavailable at boot
When ordinary context assembly or a compaction trigger executes
Then ordinary prompt assembly and unrelated turns remain available
And manual compaction returns typed unavailable state
And automatic consumers do not fabricate a plan, service owner, or generation
And host-owned bounded emergency history repair may continue without an ambient lookup or direct planner fallback

#### Scenario: Context compaction call is cancelled
Given a planning request is queued or active under one accepted generation
When caller or generation cancellation occurs
Then the request returns typed cancellation or managed generation state
And no provider call, semantic compaction fact, conversation mutation, metric, or frontend event is performed by the service
And managed call accounting settles before retirement

#### Scenario: Context compaction generation shuts down
Given the context/compaction generation owns one strict task worker
When managed shutdown closes admission
Then active calls receive one 30-second drain deadline
And cleanup receives one non-resetting 5-second deadline
And the request queue and worker stop and join before retirement
And a stale captured handle reports draining, degraded, or retired state instead of switching to another generation

#### Scenario: Two sessions share context compaction implementation policy
Given two sessions use handles for the same accepted context/compaction generation
When they have different prompt injections, TTLs, conversation histories, routes, or semantic frontiers
Then each request contains only its host-normalized immutable conversation snapshot
And each session retains independent context, conversation, provider, and semantic authority
And the service retains no session state between requests

### Requirement: Git repository and workspace operations share one managed owner

The optional release-coupled Git contribution must publish `service:git` / `interface:omegon-git-v1` as a boot-captured managed service owned by `feature:git`. One serial generation-owned worker must own the discovered repository model, libgit2 repository/index/worktree access, and every Git or JJ subprocess used by `omegon-git` for repository reads, commits, merges, worktrees, submodules, and JJ operations. Consumers must retain only the managed handle or immutable owned observations and must not perform ambient repository discovery or direct production fallback. Host invocation admission, workspace registry and lease authority, cleave scheduling, branch/message policy, and frontend presentation remain outside the service.

#### Scenario: Repository consumers share one exact generation
Given an accepted Git generation was captured at boot
When core commit tracking, repository status, cleave, and workspace controls perform Git operations
Then every operation uses the same generation-tagged handle or immutable observation from that handle
And repository-relative and workspace paths are checked against the captured boundary
And no consumer constructs `RepoModel`, opens a project repository, or calls `omegon_git` directly

#### Scenario: A Git or JJ process is cancelled
Given an admitted Git request owns a subprocess with descendants
When caller cancellation or the active-call deadline fires
Then the generation terminates the complete owned process tree
And it joins the process and descendants before settling the call
And strict cleanup cannot release repository writer ownership while a descendant remains live

#### Scenario: Host authority remains outside Git execution
Given a tool, cleave operation, or workspace command requests a Git mutation
When the host has not completed its applicable invocation, RBAC, approval, or workspace checks
Then the Git service is not invoked
And the service cannot widen paths, choose a branch or message, mutate workspace lease state, or publish host completion

#### Scenario: Candidate publication fails
Given a callable accepted Git generation and a prepared candidate
When candidate readiness, resource parity, or publication validation fails
Then the prior handle, worker, repository model, and admission state remain callable
And every candidate process and repository resource is settled or retained as degraded rollback evidence

#### Scenario: Git handle becomes stale
Given a consumer retained a Git handle from an admitted generation
When that generation becomes draining, degraded, or retired
Then the handle returns the corresponding typed managed-service error
And the consumer neither discovers another repository nor invokes a direct fallback

#### Scenario: Git service is absent
Given the optional Git service is unavailable at boot
When a Git-backed core, cleave, or workspace operation is requested
Then it returns typed unavailable evidence
And unrelated tools, sessions, frontends, and local-directory workspace operations remain callable
And no repository owner, generation, Git/JJ process, or fallback implementation is fabricated

#### Scenario: Git resources shut down
Given the Git worker owns strict process-set and repository-writer resources
When managed shutdown closes admission
Then admitted calls settle or are cancelled within the active-call deadline
And the worker stops and joins before process-set settlement
And complete process trees settle before repository writer ownership is released
And no repository handle, index lock, worktree mutation, child process, or descendant survives retirement

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

### Requirement: Runtime doctor recommends explicit process replacement

The host must expose `/doctor` and `/runtime doctor` as read-only diagnostics over the published dynamic contribution inventory and live extension supervisors. A finding for an unavailable or unhealthy extension must identify the affected contribution, state observable evidence, and recommend `/runtime replace <name>`. Doctor must not restart, replace, reload, or otherwise mutate a contribution.

`/runtime replace <name>` must perform one bounded re-instantiation from the currently admitted immutable snapshot. It must preserve the published contribution generation, host-owned schemas, and existing supervisor-backed handles. It must not inspect newly installed source bytes, retry in a loop, consume automatic restart budget, or replace unrelated contributions.

#### Scenario: Doctor finds an unavailable extension
Given a published extension supervisor has no callable child process
When the operator runs `/doctor` or `/runtime doctor`
Then the report identifies that extension as unavailable
And it recommends `/runtime replace <name>`
And no process or contribution state is mutated

#### Scenario: Operator replaces one extension process
Given an extension was published from an admitted immutable snapshot
When the operator runs `/runtime replace <name>`
Then the host stops and reaps the prior process tree
And it spawns and handshakes one replacement from the retained snapshot
And existing host bindings route to the replacement without EventBus republication
And unrelated contributions remain unchanged

#### Scenario: Replacement fails
Given a published extension cannot complete replacement startup or compatibility checks
When `/runtime replace <name>` executes
Then the failed candidate process is reaped
And that extension remains unavailable with bounded diagnostic evidence
And no automatic retry loop starts

### Requirement: Native extensions pass host-backed conformance

Every first-party native extension must pass one reusable campaign through the
production host discovery, admission, handshake, readiness, and shutdown path.

#### Scenario: Compatible extension is admitted
Given a trusted extension snapshot implements the supported native protocol
When the production host discovers and starts it
Then the host admits its declared capabilities and publishes one generation
And the extension reports its SDK identity and readiness through the shared contract

#### Scenario: Incompatible extension is refused
Given an extension omits or violates a required protocol or readiness contract
When the production host evaluates the candidate
Then the host refuses it before capability publication
And the candidate process tree is settled within the cleanup deadline

### Requirement: Real extension capabilities traverse the host

Conformance must include a real domain invocation through the host adapter and
the admitted extension process.

#### Scenario: Codescan restores search capability
Given the host has admitted the real codescan extension for a temporary workspace
When a caller indexes and searches through the host-owned codescan tools
Then the result contains the expected workspace source hit
And provenance identifies the admitted extension generation

### Requirement: Native extension cancellation is end to end

Cancellation after dispatch must cross the host, transport, and extension worker
without publishing incomplete mutation.

#### Scenario: Active extension request is cancelled
Given an admitted extension has started a mutating request
When the caller cancels that request
Then the host and extension report a cancelled outcome for the same request identity
And incomplete state is not published

### Requirement: Extension failures remain local and owned

Crash, replacement, quarantine, and shutdown must not invalidate unrelated host
or extension capabilities, and every owned process tree must settle.

#### Scenario: One extension exhausts its restart budget
Given two extensions are admitted and one repeatedly crashes
When its restart budget is exhausted
Then only the failing extension becomes quarantined and unavailable
And the other extension and kernel capabilities remain callable

#### Scenario: Host shuts down an extension tree
Given an admitted extension has spawned a descendant process
When the host refuses, replaces, or shuts down that extension
Then the direct child and every owned descendant terminate within the cleanup deadline
And no stale generation accepts a new invocation

### Requirement: Component policy is enforced before execution

An effective deny excludes a core product component before process creation,
handshake, readiness, mutable engine access, or contribution publication.
Unrelated admitted contributions remain unchanged.

#### Scenario: Codescan is disabled before boot
Given packaged component `core:codescan` is denied by effective policy
When the runtime composes its contribution generation
Then no codescan process, handshake, readiness probe, index, or database mutation occurs
And unrelated host and component contributions remain eligible

#### Scenario: Disabled component has a required dependent
Given a non-disableable contribution requires a component denied by effective policy
When the contribution graph is validated
Then runtime publication is rejected as contradictory configuration
And the denied component is not started to repair the contradiction

#### Scenario: Disabled component has only optional dependents
Given optional contributions depend on a component denied by effective policy
When the contribution graph is validated
Then the component and those optional dependents are omitted deterministically
And diagnostics identify the dependency-based omissions

### Requirement: Disabled is a typed runtime state

A packaged component denied by policy is reported as `disabled-by-policy`, not
absent, incompatible, failed, or quarantined. Its component-backed tools are not
model-callable, while direct invocation returns typed `service:disabled`
evidence with policy provenance.

#### Scenario: Model tool inventory excludes disabled codescan
Given packaged `core:codescan` is disabled by effective policy
When model-callable tools are projected
Then codescan-backed tools are excluded
And unrelated callable tools are unchanged

#### Scenario: Direct invocation reaches a disabled adapter
Given packaged `core:codescan` is disabled by the selected profile
When a CLI, ACP, or direct tool caller invokes codescan
Then the host returns typed `service:disabled`
And the response identifies `core:codescan` and the determining policy source

### Requirement: Component policy changes are generation-bound

Profile edits do not silently mutate components captured by an active session.
The new policy applies on the next runtime boot unless a separately specified
quiescent migration protocol is used.

#### Scenario: Active session remains stable after profile edit
Given an active session captured a healthy codescan component
When the operator disables `core:codescan` in the selected profile
Then the active generation remains unchanged
And the command reports that the deny takes effect after restart

#### Scenario: Re-enabled packaged component starts after restart
Given packaged `core:codescan` was disabled without being uninstalled
And effective policy is changed to allow it
When a new runtime boot composes the generation
Then the packaged component passes normal admission and readiness
And codescan search becomes available without reinstallation

### Requirement: New extension generations activate at quiescent boundaries

Newly installed extension bytes may be discovered, admitted, staged, and
published without restarting the host, but publication must occur only at a
quiescent runtime boundary. Active work retains its captured generation, stale
handles cannot gain authority in the new generation, and failed staging leaves
the active generation unchanged.

#### Scenario: Candidate arrives during an active turn
Given a session has captured the active contribution generation
And a newly installed extension candidate passes discovery and admission
When activation is requested during the active turn
Then publication remains pending until the runtime is quiescent
And the active turn cannot observe the candidate generation

#### Scenario: A newer candidate supersedes a pending candidate
Given generation A is active and generation B is staged pending quiescence
And admitted generation C arrives for the same contribution
When the lifecycle owner accepts C
Then B is removed from candidate state only after all B-owned resources settle
And C becomes the sole pending candidate without changing A

#### Scenario: Quiescent activation succeeds
Given an admitted candidate generation is fully staged and the runtime is idle
When the supervisor publication coordinator explicitly commits the candidate
Then new work captures the new generation without host restart
And superseded processes and handles settle under bounded lifecycle ownership

#### Scenario: Turn completion does not imply publication
Given a changed generation is pending during an active turn
When that turn closes and another turn is requested before an explicit coordinator commit
Then the pending generation remains hidden and the next turn retains the active generation
And only a later quiescent commit can publish the candidate

#### Scenario: Stale extension authority survives in a caller cache
Given generation B has replaced generation A
And a caller retains an A-bound invocation lease or polling handle
When the caller attempts native RPC through the retained authority
Then the shared generation fence denies dispatch before owner entry
And fresh admission resolves B while aliases and caches cannot revive A

#### Scenario: Candidate staging fails
Given the active generation is healthy and a candidate fails probe or staging
When activation settles
Then the prior generation remains published and callable
And no partial schemas, actions, routes, or processes from the candidate become visible

#### Scenario: Remote cleanup cannot be observed
Given a remote contribution has host-owned transport resources and unobservable remote state
When cancellation, replacement, or shutdown cleanup reaches its deadline
Then every host-owned resource settles within its declared boundary
And remote cleanup is reported as best-effort or unverified rather than strict success
