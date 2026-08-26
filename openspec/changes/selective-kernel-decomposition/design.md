# Design: selective Omegon kernel decomposition

## Source design

- `docs/selective-kernel-decomposition.md`
- `docs/omegon-maintain.md`
- `docs/binary-composition-and-kernel-admission.md`
- `docs/harness-architecture-parity/deepseek-harness.md`
- `openspec/archive/runtime-capability-declarations/`

## Decision

Adopt "everything optional attaches through a contract," not "everything is an ordinary plugin."

The constitutional kernel owns only identity, contribution lifecycle, durable session transition truth, admission combination, invocation leases, event ordering, process/resource ownership, and recovery bootstrap. Default loop behavior, providers, tools, memory, lifecycle, planning, context policy, orchestration, and frontends are system modules or selectively composed services. Third-party code remains separated by explicit trust and transport boundaries.

## Lifecycle status

This change remains `proposed` overall. Slices 0 through 5 are complete; optional domain extraction remains open. Slice 5.5 gives all 54 rows exhaustive scenario-specific executors, includes AC13's chunk-bearing mixed-lineage rebuild fixture, and passes the focused Ubuntu/Windows campaign matrix. The macOS, Ubuntu, and Windows campaigns are evidenced within budget; GitHub Actions run `32622078435` at `b788f3b8` supplies the required Ubuntu and Windows evidence. Slice 5.6 closes compatibility publication at the frozen semantic self-sufficiency boundary, migrates maintenance to catalog-first framing, and publishes the applicable public/developer documentation and canonical snippets. Slices 6.1.1 through 6.1.5 implement atomic no-resource typed-service publication and cut over the plans/work and stateless behavior-policy lanes. Slice 6.1.6 implements managed drain/cleanup, Slice 6.1.7 publishes codescan as the first resource-bearing service, and Slice 6.1.8 completes lifecycle/OpenSpec as one revisioned managed lane with crash-recoverable design and OpenSpec transactions, strict resource settlement, typed absence, and production consumer cutover to exact-generation handles or immutable observations. Its portable campaign, direct-owner/write source guard, and reconciled documentation are complete; GitHub Actions run `32856627018` at `ff56bed3` supplies green native lifecycle evidence for macOS, Ubuntu, and Windows. Slice 6.1.9.0 freezes memory as the next managed lane without changing runtime behavior. Slice 6.1.9.1 hardens schema v8 persistence, payload-bound operation replay, transactional compound mutations, Lamport and backend parity, deterministic fallback, and governed migration before managed publication. Homebrew publication is not a Slice-zero exit gate; existing formula verification remains a best-effort packaging safeguard. Pairwise install, self-update, and version switching publish immutable complete generations and select the executable pair plus receipt through one atomic activation link. Each later slice begins with an explicit refinement gate that names concrete ownership, compatibility boundaries, red tests, and documentation impact before production mutation.

Slice 6.1.9 is complete. Memory persistence, filesystem synchronization, and production consumers now use one managed durable service with strict settlement and typed absence. Its portable campaign and direct-owner/write source guard pass on macOS, Ubuntu, and Windows in GitHub Actions run `32936194406` at `a4b14499`.

Slice 6.1.10 completes context/compaction as a managed deterministic-planning lane. The service receives immutable host-normalized conversation entries and owns only eligibility, keep-window selection, evicted-entry counts, reasons, and provider-payload formatting. Session-owned `ContextManager` state, canonical conversation mutation, semantic compaction authority, supervisor admission, provider routing, metrics, and frontend events remain outside the service. This boundary is optional, boot-captured, exact-generation, cancellation-aware, and backed by one strict task worker; absence never triggers ambient lookup or direct planner fallback. Git remains the only unfinished task-6.1 domain.

Slice 6.1.11 freezes Git as the final task-6.1 managed lane. `feature:git` publishes optional boot-only `service:git` / `interface:omegon-git-v1` under contribution generation `contribution:git-managed-v1`. One serial worker owns the repository model, libgit2 repository/index/worktree mutation, and every Git or JJ subprocess launched by `omegon-git`. Consumers receive only a boot-captured exact-generation handle or immutable boot observation; requests name repository-relative paths or host-approved workspace paths contained by the captured repository/workspace boundary and return owned DTOs.

The Git service does not absorb host authority. Invocation admission, RBAC and approval, tool schemas and rendering, workspace registry/lease state, cleave scheduling, branch and message policy, child lifecycle, and process shutdown ordering remain host-owned. The service executes only an already-admitted typed operation. Package/extension installation clones, updater and release probes, TDD evidence reads, and toolchain diagnostics remain separate host operations because they neither use the captured project repository nor express `omegon-git` repository/workspace semantics.

The generation owns strict `resource:git-worker`, `resource:git-process-set`, and `resource:git-writer` resources. The worker stops and joins before the process set settles, and all complete process trees settle before repository writer ownership is released. Git/JJ children use argument arrays and an owned process group on Unix or an owned Job Object/equivalent tree boundary on Windows; cancellation, active-call deadline expiry, candidate rejection, shutdown, and owner drop request tree termination and join rather than leaving detached descendants. No successful mutation may be reported after cancellation unless the atomic libgit2 mutation had already committed, in which case the response is settled before the worker accepts another request.

If Git is absent, core Git operations, cleave repository operations, and Git/JJ workspace creation return typed unavailable evidence while unrelated tools, sessions, local-directory workspaces, and frontends continue. Consumers do not rediscover a repository, construct `RepoModel`, call `omegon_git` directly, or spawn a fallback Git/JJ command. Explicit direct fixtures remain permitted in tests. Candidate failure preserves the prior generation, unchanged exact-generation transfer retains the same physical worker, and stale handles return managed draining, degraded, or retired errors.

Slice 6.2 makes semantic surfaces an owner-neutral projection boundary rather than a second integration layer. Modules under `src/surfaces/` may depend on shared contracts and other semantic DTOs, but they do not import feature implementations, managed-service implementations, lifecycle/session stores, provider registries, credential probes, filesystem/process APIs, or renderer/protocol types. Concrete owners produce immutable input snapshots and action descriptors before invoking surface projection. This keeps implementation enum matching, source discovery, health probing, and authority decisions at the producer boundary.

The minimum shared vocabulary is additive and preserves current frontend wire shapes: operation/activity snapshots, task-identity findings, memory/federation observations, diagnostics, provider/model/profile/settings inputs, session projection inputs, and canonical action availability. Existing DTO names remain where they are already owner-neutral; types move to a neutral owner only when more than one producer or edge needs them. TUI, ACP, Web, IPC, CLI, and daemon consume the same projections/actions, then apply only transport serialization, redaction, and support narrowing. No edge may recover missing data by probing an implementation owner or duplicate projection/availability policy.

The 6.2 campaign fixes three compatibility laws. First, equivalent producer snapshots yield equivalent semantic fields and action availability at all six edge families. Second, existing serialized field names, enum strings, omission/null behavior, and command identities remain unchanged. Third, source guards reject concrete production imports and ambient filesystem, process, credential, or registry probing from `surfaces/`; test fixtures may construct concrete owner state only outside the guarded production body. Dynamic discovery, content-pack extraction, release locks/budgets, durable session schema changes, and unrelated behavior remain assigned to later tasks.

## Architectural layers

### Constitutional kernel

The kernel owns:

- stable runtime, contribution, capability, session, prompt, turn, invocation, lease, and generation identities;
- renderer-neutral protocol and semantic event vocabulary;
- contribution dependency validation, generation construction, promotion, drain, rollback, and retirement;
- one session state machine and supervisor implementation, instantiated once per session;
- monotonic admission combination and privileged execution leases;
- crash-consistent invocation state;
- semantic event sequencing, snapshot cursors, and terminal closure;
- host-owned process/resource supervision and machine-readable recovery diagnostics.

These are release-only authorities. They may be split into internal crates, but they are not ordinary operator-installed plugins.

### Release-coupled system modules

System modules use the same declaration and lifecycle vocabulary while retaining privileged, in-tree status:

- the separately runnable `omegon-maintain` companion executable;
- the default loop policy driver;
- provider route selection and route leases;
- provider-history and transcript projections;
- filesystem, process, network, terminal, secret, and resource host-effect executors;
- required configuration and packaging adapters.

Replacement occurs at boot or a declared quiescent generation boundary, never mid-turn.

### In-process services

Memory, lifecycle, Git, codescan, context/compaction, behavior policy, plans/work aggregation, provider transports, and catalog resolution are statically linked Rust services with explicit contracts, dependencies, and teardown. Extraction from the main crate does not grant third-party replacement authority.

Slice 6.1 distinguishes these services from constitutional `kernel_service` capabilities. A declared in-process service is a bindingless capability paired with exactly one typed implementation in the same candidate generation. Candidate graph validation, implementation parity, dependency activation, readiness, and publication of the typed registry are one atomic boundary. A published handle identifies its capability, owner, and contribution generation; consumers capture it at boot or a separately admitted quiescent boundary rather than looking up a changing ambient implementation mid-turn.

Required and optional service dependencies participate in graph activation and health. Missing or failed optional services publish bounded unavailable/degraded evidence and leave unrelated capabilities callable; they are never represented as active merely because their declaration was accepted. Candidate failure preserves the complete previous graph and service registry. Retirement settles every generation-owned task, subscription, temporary artifact, and durable writer before claiming success. Services with no owned resources declare strict no-resource teardown rather than inheriting a best-effort process cleanup default.

The first proof lane is read-only plans/work aggregation. `styrene-work-model` remains the provider-neutral contract owner and `styrene-work-runtime` remains the refresh/immutable-snapshot owner. Omegon contributes OpenSpec/design source adapters and semantic projection integration. Source errors become source-local warnings instead of aborting the aggregate. The service may be absent without removing session-local plan mutation. This lane does not move `IntentDocument` plan authority, change Workbench or `PlanSurfaceProjection` DTOs, revise durable session schemas, or alter commands and frontend wire behavior.

Slice 6.1.1 publishes only the substrate needed by that lane. The additive service capability carries a stable interface identity and an object-safe implementation holder. EventBus publishes typed handles only after graph and implementation parity pass and rejects any capability, interface, erased type, implementation, or service-set change that reuses an accepted contribution generation. The initial API is intentionally limited to strict zero-timeout no-resource reads. Existing handles may retain their captured immutable generation; tasks, subscriptions, durable writers, and other resource-bearing services cannot use this path until contribution lifecycle retirement supplies real drain and cleanup evidence. No production domain is registered by 6.1.1.

Slice 6.1.2 registers the first production half of that lane. The statically linked `work-aggregation` contribution performs one boot refresh, normalizes OpenSpec and design artifacts, and publishes an immutable `WorkSnapshot` through `service:work-snapshot` / `interface:styrene-work-snapshot-v1`. It owns no task, subscription, watcher, or durable writer. Existing CLI, loop, and ACP repository-plan readers remain on their compatibility scanners until the next bounded shared-reader cutover; therefore service absence is not yet claimed as a complete plans/work extraction.

Slice 6.1.3 adds the semantic conversion boundary without changing consumers. Source facets retain the OpenSpec grouping/order and structured task-identity findings needed to recreate the existing repository plan/task DTOs; duplicate stable markers remain diagnosable while normalized work identities stay unique. The shared `surfaces::plans` adapter has parity coverage against the direct scanner, so the following cutover can replace producer access without changing command or frontend shapes.

Slice 6.1.4 captures the typed work snapshot immediately after accepted composition publication and carries that immutable handle through every loop host and the ACP worker boundary. Shared CLI, loop, and ACP repository-plan readers consume only the captured snapshot. Absence is explicit: session-local `IntentDocument` mutation and rendering continue, repository plans/tasks are empty, and no reader performs an ambient service lookup or direct filesystem fallback. The compatibility scanner remains only as the parity oracle for tests.

Slice 6.1.5 is the second no-resource proof lane: stateless behavior policy. The release-coupled `behavior-policy` contribution publishes `service:behavior-policy` / `interface:omegon-behavior-policy-v1` as an optional synchronous service captured once after accepted composition. Its object-safe contract consumes immutable host-normalized per-turn views and computes only advisory unpinned task-mode inference, phase, drift, progress/evidence, first-turn/execution/continuation pressure, pressure/meta messages, substantive-prose classification, and pathological-meta-response decisions. It retains no conversation, tool, controller, frontend, or durable-session state. Explicit operator mode parsing and operator-correction recovery, declared tool capabilities, authoritative observation normalization, `IntentDocument`, persisted `TaskMode`, `ControllerState`, stuck/dead-mouse/meta counters, tool execution, event emission, and nudge insertion remain session/host authority. Dynamic tool declarations and normalized observations remain per-turn host input rather than service-owned state.

Normal composition remains parity-equivalent across canonical fixtures BP01-BP09. Their canonical baseline is the direct implementation and assertions at commit `9c3a9860`; task 6.1.5 materializes shared direct/service vectors plus literal enum, boolean, and exact-message expectations that survive deletion of direct production calls.

| Fixture | Frozen baseline cases |
|---|---|
| BP01 | `infer_task_mode_classifies_research_and_implementation_prompts` and `observed_task_mode_does_not_override_pinned_mode`; explicit pinned modes bypass the service. |
| BP02 | `classify_turn_phase_treats_validate_tool_as_act` plus direct `classify_turn_phase` vectors for empty, orientation-only, repository-inspection, mutation, validation, and mixed calls yielding `None`, Observe, Orient, or Act. |
| BP03 | `classify_drift_kind_does_not_flag_single_targeted_read_as_orientation_churn`, `classify_drift_kind_flags_broad_inspection_loop_as_orientation_churn`, `classify_drift_kind_requires_similar_failed_mutations_for_repeated_action_failure`, `classify_drift_kind_flags_repeated_failures_on_same_path`, and `classify_drift_kind_does_not_flag_targeted_validation_as_validation_thrash`, extended with closure-stall. |
| BP04 | `classify_progress_signal_recognizes_constraint_discovery_from_new_constraints` and `classify_progress_signal_ignores_unevidenced_constraint_growth`, extended with literal mutation, commit, targeted/broad validation, and no-progress vectors from host-normalized observations. |
| BP05 | `evidence_assessment_splits_local_and_global_after_targeted_validation` and `evidence_assessment_keeps_narrow_local_archaeology_out_of_global_sufficiency`, extended with none/actionable and research/implementation vectors. |
| BP06 | `first_turn_orientation_churn_detected_for_headless_execution_bias_mode`, its real-inspection and normal-mode negatives, and the existing `execution_pressure_*` positive/negative tests. |
| BP07 | `continuation_pressure_relaxed_but_not_disabled_in_research_mode`, sustained/resumed churn, research, Act, bash, slim, and constrained-pressure tests. |
| BP08 | `substantive_prose_threshold_separates_narration_from_analysis` and `substantive_prose_holds_continuation_counter`; the service classifies prose and the caller alone updates counters. |
| BP09 | `continuation_pressure_messages_prohibit_meta_recovery`, `evidence_and_local_first_messages_prohibit_meta_recovery`, and `pathological_meta_response_detects_self_rebuke_without_progress`, extended with literal exact strings for standard/constrained first-turn and execution-pressure guidance, every pressure tier, evidence, local-first, and meta-retry output. |

If the optional service is absent, ordinary text and tool turns remain callable, explicit and existing session intent are preserved, controller and recovery counters are held rather than advanced from synthetic no-progress, and only behavior-policy-derived first-turn/execution/evidence/continuation nudges and meta retry are disabled. Host-owned operator-correction recovery, completion reconciliation, plan reminders, stuck recovery, and text-only recovery remain unchanged. Switched consumers neither look up the registry mid-turn nor call a direct behavior fallback. `LoopCompatibilityBindings` carries `Option<BehaviorPolicyBinding>`: a present binding retains capability, owner, generation, and implementation; absence is `None` plus graph unavailable/degraded evidence and never fabricates service owner or generation identity. Interactive, daemon/control, headless, bounded, Sentry, and ACP execution carry the same optional binding, including ACP worker transfer; each session retains independent controller and recovery state. The service declares strict zero-timeout no-resource teardown and owns no task, subscription, process, temporary artifact, or durable writer. Resource-bearing memory, lifecycle, context/compaction, codescan, and Git lanes remain deferred until generation-bound drain and cleanup are implemented.

Slice 6.1.6 establishes that resource-bearing prerequisite without migrating a production domain. The existing `no_resource_read_service` constructor remains a separate raw-`Arc`, strict zero-timeout class: immutable/stateless captured handles may retain their original implementation, and work snapshot plus behavior-policy behavior is unchanged. A managed declaration instead carries at least one generation-owned resource controller, `DrainExisting`, nonzero active-call and cleanup deadlines, and an implementation that never escapes as a raw `Arc`. The object-safe `ManagedServiceContract` fixes associated `Request`, `Response`, and `Error` types and exposes `fn execute<'a>(&'a self, request: Self::Request, context: ManagedCallContext) -> Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send + 'a>>`; `ManagedServiceHandle<S>::invoke(S::Request)` privately dispatches that future in a generation-owned call task and returns `Result<S::Response, ManagedServiceCallError<S::Error>>`. `Request`, `Response`, and `Error` are `Send + 'static`, the service is `Any + Send + Sync + 'static`, and registration exposes only a trait object with all associated types fixed. `ManagedCallContext` contains only cancellation plus captured capability/owner/generation identity. `ManagedServiceCallError` distinguishes operation error, cancelled, panicked, generation draining, generation degraded, and generation retired. No consumer API exposes `&S`, `Arc<S>`, downcasting, or an implementation-returning callback. Admission and active-call guards settle on success, error, cancellation, and caught panic. Synchronous unbounded work is ineligible unless moved into a declared cooperative task, and every spawned descendant must register as a generation resource.

Candidate managed resources register under exactly one `(contribution, contribution-generation)` before readiness and remain invisible until promotion. Graph, implementation, resource, lifecycle, cleanup dependency, and policy parity are validated together. Every pre-publication failure closes candidate admission and settles or retains only candidate resources while the complete prior graph, registry, handles, and resources remain callable. All managed handles consult one shared `ManagedAdmissionRegistry` and their immutable `(contribution, generation)` key while briefly holding its read lock to admit and count a call. Whole-graph promotion prepares one immutable table containing unchanged generations as accepting, replaced/removed generations as draining, and new generations as accepting. After all fallible preparation, replacing that table under one write lock is the global publication linearization point: a racing call is admitted against the old table or observes the complete new table, never a partially closed set. Publication is irrevocably committed there, and the candidate graph/registry/generation swaps complete through non-failing in-memory assignments before the exclusive EventBus mutation returns. Pre-point failure returns `Rejected { rollback_cleanup }`; post-point completion returns `Published { generation, retired_cleanup }`, even when old cleanup degrades.

Every admitted managed call runs in the generation-owned task set. The active-call deadline begins at the admission-table swap or shutdown-table closure. At expiry, generation cancellation fires and remaining call tasks are aborted and joined; panic and uncooperative cancellation are degraded evidence, never detached work. Only after all call tasks are joined does a separate monotonic cleanup deadline begin. Candidate rollback has no published calls and begins cleanup immediately. Resource controllers expose idempotent `request_stop`, `force_stop`, and repeatable `await_settled`; their frozen dependency DAG defines cleanup so a resource is stopped, forced when its bounded cooperative settlement fails, and awaited before any resource it depends on. The same reverse-topological order governs stop, force, and settlement; missing dependencies and cycles reject the candidate. When no await budget remains, remaining controllers still receive stop then force in that order and stay retained as degraded/unverified. The deadline is never reset per resource, and unresolved controllers remain owned for retry.

Strict cleanup still requires positive settlement before `CleanupSettled`, `Retired`, or ownership release. A strict timeout/failure records `Degraded` with verified owner/resource identity, stop and force-stop attempts, and a bounded diagnostic, but this is a nonterminal cleanup state rather than verified settlement. Best-effort cross-boundary cleanup may be `Unverified`. Degraded/unverified generations stay retained at `CleanupStarted`; a later retry may advance them to `Retired`. Resource records keep their existing schema and kinds in this substrate; synthetic temporary state uses `TemporaryDirectory`, so no new durable enum or wire value is introduced.

A rejected candidate whose cleanup settles records `Failed` at `CleanupSettled`; if cleanup remains unresolved it records `Degraded` at `CleanupStarted` until retry settles, then returns to the rejected `Failed` record without ever becoming active or retired. A replaced active generation follows `Active -> Draining -> Retired` only on settlement, or `Active -> Draining -> Degraded` while ownership remains unresolved.

After the first accepted composition, changed or newly introduced `Boot` services are always rejected before any active gate closes. A service declared `QuiescentSession` requires a current-composition, runtime/session-bound, one-use proof that no turn, unresolved invocation, or managed call is active; stale, cross-session, or replayed proofs fail closed. Slice 6.1.6 defines test-only proof issuance and validation but ships no production issuer, command, or migration flow. `ProjectionBoundary` cannot replace managed services. EventBus owns all candidate, active, retiring, and unresolved generations plus their cleanup tasks; replacement and shutdown serialize through that owner, and caller cancellation cannot detach retirement. Semantic diagnostics retain bounded DTO-only published/rejected history and project actual active, draining, degraded, cleanup, and retired records rather than synthesizing `NotRequired`. Exact-generation transfer preserves the physical owner's original resource-admission composition identity while the containing projection identifies the current accepted composition. Interactive, daemon, headless, bounded, Sentry, ACP worker, cleave, and injected-runtime hosts explicitly await managed shutdown before releasing runtime ownership; unresolved strict or best-effort resources remain retained for retry.

Explicit asynchronous shutdown closes gates, joins calls, and runs the same cleanup engine. Process-level runtime ownership is removed only after strict settlement. If shutdown ends degraded, it records a final degraded heartbeat and leaves the ownership record for maintenance/stale pruning rather than claiming clean exit; Drop only requests cancellation/force-stop and cannot report settlement or remove that evidence.

The synthetic campaign freezes RG01 candidate rollback with prior-registry preservation; RG02 gate/publication races; RG03 active-call completion, cancellation, panic, and uncooperative-call joining; RG04 stale-handle states; RG05 unchanged-generation transfer; RG06 controlled-clock active and cleanup deadline accounting; RG07 dependency-DAG order/cycle rejection; RG08 strict degradation, retained retry, and later retirement; RG09 best-effort cross-boundary unverified cleanup; RG10 replacement/shutdown serialization and caller-cancellation safety; RG11 diagnostic projection and every normal host's explicit shutdown path; and RG12 no-resource work/behavior regressions. Codescan follows separately because it is rebuildable, workspace-scoped, and has no required subprocess or session authority. Its lane must first prove one SQLite owner across tools and `request_context(kind="code")`, transactional canceled path updates, owned/joined HEAD work, connection closure, stale-handle denial, typed absence, and unrelated-context continuity. Memory, lifecycle, Git, and context/compaction remain deferred.

Slice 6.1.7 promotes codescan as the first production managed service. The stable identities are `feature:codescan`, `service:codescan`, `interface:omegon-codescan-v1`, and contribution generation `contribution:codescan-managed-v1`. Activation is boot-only. The optional service remains in tool inventory when absent and returns typed unavailable evidence. `request_context(kind="code")` reports the code part unavailable while unrelated requested context kinds continue. Consumers capture one `ManagedServiceHandle<CodescanService>` at boot and never open `ScanCache`, invoke `Indexer`, look up the ambient registry, or retain the implementation.

One serial blocking worker exclusively owns the SQLite connection, filesystem scan, tree-sitter parsing, HEAD freshness checks, and BM25 construction. Managed requests carry operation cancellation into queued and active worker commands. No detached HEAD task remains. The worker is `resource:codescan-worker` with strict `Task` assurance and depends on `resource:codescan-writer`, a strict `DurableWriter`. Cleanup stops and joins the worker before it awaits writer settlement; writer settlement means the connection and WAL handles have closed. Codescan declares a 30-second active-call drain deadline and a 5-second cleanup deadline. It owns no best-effort or cross-boundary resource.

All persisted paths are repository-relative before hash, stale, live, prune, and row-state comparison. A dedicated file-state table records the content hash and kind even when a file produces zero chunks. Incremental indexing prepares parsing outside SQLite, checks cancellation, and commits each complete path replacement atomically; already committed paths may remain updated if a later path is cancelled. Missing-path pruning and `last_head` advance only after a complete successful incremental run. `invalidate=true` clears, rebuilds, prunes, and advances metadata inside one transaction, so cancellation or failure preserves the complete prior searchable index. Empty results, service absence, cancellation, SQLite failure, and managed stale-generation errors remain distinct outcomes.

Compatibility fixtures retain the existing tool names, parameters, capabilities, `within` containment and filtering, result details, BM25 ranking, code-context diversity, and Java/Kotlin/C# behavior. The real-domain campaign adds one-writer concurrency, transactional code and knowledge replacement, cancelled full invalidation, zero-chunk file state, dirty/non-Git relative-path stability, candidate rollback, exact-generation worker transfer, stale-handle denial, strict worker-before-writer cleanup, Windows reopen/delete closure evidence, typed absence, mixed-context continuity, and source guards that prohibit direct production `ScanCache::open` or `Indexer::run` outside the service owner. No command syntax, durable session schema, configuration schema, or canonical snippet changes.

Slice 6.1.8 freezes lifecycle/OpenSpec as the next managed lane. The stable identities are `feature:lifecycle`, `service:lifecycle`, `interface:omegon-lifecycle-v1`, and contribution generation `contribution:lifecycle-managed-v1`. Activation is optional and boot-only. One serial repository worker owns the loaded opsx FSM and JSON ledger, design/OpenSpec artifact inspection, revision coordination, reconciliation, journal recovery, and every Omegon-authored lifecycle mutation. Consumers retain only a boot-captured `ManagedServiceHandle<LifecycleService>` or immutable DTOs returned by that handle. They never receive `Lifecycle<JsonFileStore>`, `JsonFileStore`, `OpenSpecRepository`, a design repository/provider lock, or an implementation-returning callback, and they perform no ambient registry lookup or direct filesystem fallback.

Git-native design and OpenSpec artifacts remain canonical semantic content. The opsx state store remains an enforcement and audit ledger rather than a second content authority. A service repository revision identifies the selected design root, selected OpenSpec root, ledger state, and lifecycle transaction frontier. Read requests return owned design/OpenSpec snapshots, node/change queries, readiness/blocking/frontier projections, task-ID findings, artifact health, ledger state, and drift with explicit absent, malformed, unreadable, stale, and recovery-required states. Mutation requests carry a stable operation identity and expected repository revision and cover typed design creation/transitions/questions/research/decisions/links/implementation metadata/branching/implementation plus OpenSpec proposal, spec addition, task reconciliation/status, test registration, lifecycle transition, archive, abandon/reopen, and explicit recovery. A successful mutation returns the committed revision and resulting projection; clients do not rescan independently. Stale revisions fail before mutation. Replaying a committed operation identity returns its recorded outcome rather than applying it twice.

The lifecycle service does not absorb session or host authority. Focused-node selection, focus/unfocus commands, context TTL and turn counters, pending memory requests, tool JSON and rendering, command registration, admission/RBAC, frontend presentation, work scheduling, and process shutdown remain session/host owned. Human or agent authoring of arbitrary design, task, and spec prose remains an external Git-native workflow; an observed external edit invalidates the prior revision and must be parsed, health-checked, and reconciled before the next managed mutation. TDD evidence remains an adjacent append-only namespace, Codex export remains derived output, and filesystem-layout migration remains a stopped-runtime maintenance operation. The selected OpenSpec root follows the repository path policy once at service startup; simultaneous populated primary and legacy roots are a typed conflict, not two merged authorities.

The worker is strict `resource:lifecycle-worker` (`Task`) and depends on strict `resource:lifecycle-writer` (`DurableWriter`). It owns no subprocess or best-effort cross-boundary resource. Active calls drain for 30 seconds and cleanup has one non-resetting 5-second deadline. Settlement means the request queue and worker have stopped and joined, all temporary files and journal operations are resolved or retained as explicit recovery-required evidence, no write handle remains open, and artifact/ledger parent-directory durability has completed. Cleanup stops and joins the worker before writer settlement. Strict timeout or failure remains degraded with retained ownership and cannot claim retirement.

Every mutation validates from immutable current artifact and ledger state before publication. In-memory FSM changes are staged or restored when persistence fails. Single-file replacement uses unique transaction-local temporary files, file durability, atomic rename, and parent-directory durability. Multi-resource design/artifact/ledger mutations use a versioned, checksummed, repository-relative, path-contained journal and deterministic recovery so restart yields a defined complete pre-operation or post-operation state, never a confidently successful partial prefix. Archive recovery validates repository identity, journal version, operation, phase, paths, and content identities; one corrupt journal is quarantined as typed recovery-required evidence rather than making unsafe guesses. A lock covers revision comparison through commit, and unsupported non-owning lock semantics fail managed mutation closed rather than allowing last-writer-wins replacement.

If the optional service is absent, lifecycle tools and semantic surfaces remain declared but return typed unavailable state, lifecycle-derived context is omitted, and unrelated context, tools, sessions, and frontends continue. Work aggregation and ACP receive absent/degraded lifecycle input rather than constructing independent scanners or ledgers. Canonical parity fixtures retain existing tool names and arguments, design/OpenSpec DTO fields, transition policy, task counting and stable-ID behavior, ACP methods, TUI/Web/IPC projections, and context semantics when present. The real-domain campaign adds failed-save rollback, revision conflict and idempotent replay, external-edit invalidation, corrupt/future state, primary/legacy root conflict, multi-file crash recovery, archive path tampering, concurrent clients, candidate rollback, exact-generation transfer, stale-handle denial, strict worker-before-writer cleanup, Windows reopen/rename/delete evidence, typed absence, and source guards against production direct owners or writes outside the frozen exclusions.

Slice 6.1.9 freezes memory as the next managed lane. The stable identities are `feature:memory`, `service:memory`, `interface:omegon-memory-v1`, and contribution generation `contribution:memory-managed-v1`. Activation is optional and boot-only. One serial worker owns the selected project store, optional global store, SQLite connections and WAL state, durable facts, minds, edges, episodes, vectors, JSONL import/export, and configured Codex-vault synchronization. Consumers retain only a boot-captured `ManagedServiceHandle<MemoryService>` or owned DTOs. They never receive `Arc<dyn MemoryBackend>`, a SQLite connection, a vault writer, or a callback that exposes the implementation, and they perform no ambient registry lookup or direct persistence fallback.

The versioned request contract covers availability and statistics, fact and episode reads, FTS/vector/hybrid retrieval, mind-scoped graph reads, durable fact/edge/episode/vector mutations, JSONL import/export, vault synchronization, and bounded maintenance. Responses return owned domain DTOs, bounded diagnostics, and typed effects or errors. Mutations carry stable operation identity. Content-addressed fact stores remain naturally idempotent. A targeted archive, supersede, reinforce, edge, or vector mutation uses entity-specific identity and version preconditions when required. Independent facts do not contend on one artificial global revision. JSONL import retains the existing per-record Lamport conflict and deterministic tie-breaking policy. SQLite mutations that span records remain atomic, and vector failure cannot make the deterministic non-vector result unavailable.

The memory service does not absorb session or provider authority. Session-local working-memory pins, selected mind, context TTL and turn counters, context hashes and presentation policy, tool JSON and rendering, command registration, admission/RBAC, frontend state, model/provider selection, extraction prompts, embedding computation, and compaction remain session/host owned. Durable fact, edge, episode, and vector isolation by mind label remains service-owned; requests carry the host-selected mind scope. The managed version-1 contract does not add a standalone durable mind-record or parent-mutation API. Provider-backed extraction and embedding tasks must be host-owned, bounded, and joined before managed shutdown. They may submit facts, episodes, or vectors only through the captured service handle. Stopped-runtime schema and selected-root migration remain maintenance operations. One-shot embedding backfill constructs and shuts down a bounded managed composition instead of opening the database directly.

The worker is strict `resource:memory-worker` (`Task`) and depends on strict `resource:memory-writer` (`DurableWriter`). Active calls drain for 30 seconds and cleanup has one non-resetting 5-second deadline. Settlement means the queue and worker have stopped and joined, host-owned result producers can no longer submit work, SQLite connections and WAL handles are closed, JSONL and vault writes have completed or returned typed failure, and no write handle remains open. Cleanup stops and joins the worker before writer settlement. Strict timeout or failure remains degraded with retained ownership and cannot claim retirement.

Existing selected-root and legacy compatibility, schema migration, persisted and wire vocabulary, SQLite/in-memory behavior, FTS fallback and ranking, vector dimensions and model metadata, decay and reinforcement, minds and inheritance, graph and episode behavior, JSONL stability and Lamport merge, vault containment and idempotency, tool names and arguments, context ordering, and status projections remain parity requirements. If the optional service is absent, memory tools and status surfaces remain declared with typed unavailable evidence, durable memory context is omitted, and unrelated context, sessions, frontends, and host-owned compaction continue. No consumer opens a project/global store, reads JSONL, writes the vault, or fabricates a service owner. The real-domain campaign adds atomic rollback, operation replay, concurrent independent and targeted mutation, candidate rollback, exact-generation transfer, stale-handle denial, strict worker-before-writer cleanup, Windows reopen/rename/delete evidence, typed absence, deterministic no-vector behavior, JSONL/vault idempotency and path safety, cross-surface parity, and source guards against production direct owners or writes outside stopped-runtime migration.

Checkpoint 6.1.9.2 publishes the optional boot candidate and captures its exact-generation handle. The worker serializes project and explicitly configured existing-global requests, accepts provider-computed vectors only as durable mutation input, and owns its SQLite/WAL handles through strict worker-before-writer settlement. Missing or uninitialized global files are typed absence and are never initialized by discovery. Cancellation before execution removes queued work; after an atomic mutation starts, the worker settles it and stable operation replay recovers the exact outcome. This checkpoint does not yet claim sole production store ownership: compatibility consumers retain their existing backend until 6.1.9.4, while JSONL and vault ownership move in 6.1.9.3.

Checkpoint 6.1.9.3 moves project JSONL and configured Codex-vault filesystem effects behind that worker. One selected memory root supplies both the project database and JSONL path; the existing non-child empty-store bootstrap completes before readiness, while explicit import/export requests remain bounded and project-only. JSONL import participates in payload-bound replay without changing per-record Lamport conflict rules. Vault synchronization activates only from the explicit integration files, validates and bounds one non-symlink root, snapshots complete contained inputs before mutation, and uses stable note/mind/content/predecessor identities for convergent import, reinforcement, supersession, alias restoration, and replay. Deterministic section, index, and grouped daily-episode output is compared before synced atomic replacement. Cancellation is cooperative between atomic sub-operations, and worker/writer settlement covers every database, JSONL, vault, and temporary-file handle. Concurrent hostile path replacement remains outside the filesystem trust boundary; static traversal and symlink escape fail closed. Compatibility tools, context, status, lifecycle/session producers, provider-result writers, and embedding backfill retain direct backend access until 6.1.9.4, so sole SQLite ownership is still not claimed.

Checkpoint 6.1.9.4 removes that live compatibility backend. Tools, context, lifecycle ingestion, session-end persistence, embedding-result writes, status projections, and one-shot backfill use the boot-captured exact-generation binding or a bounded managed composition. Session-scoped operation identities bind the full semantic payload, targeted mutations retain mind and fact-version preconditions, and insertion-stable pagination bounds maintenance without truncating large stores. Host-owned extraction and embedding run in tracked bounded phases; finalization closes admission, cancels and joins those tasks, and only then drains the managed worker. Managed status snapshots remain project-root scoped and preserve JSONL authority, index freshness, and typed absence without reopening storage. Source guards reserve direct storage, JSONL, and vault ownership for the managed worker, tests, and explicit stopped-runtime migration. The cross-platform real-domain campaign remains 6.1.9.5.

Checkpoint 6.1.9.5 completes the portable real-domain campaign and documentation reconciliation. Five serial campaign cases plus the ownership guard cover persistence and reopen, mind-label isolation, vector metadata, graph and episodes, JSONL recovery, vault convergence and containment, deterministic no-vector fallback, concurrency and replay, active cancellation, typed absence, context/status parity, candidate rollback, exact-generation transfer, stale handles, strict worker-before-writer cleanup, and native file replacement/reopen/delete behavior. GitHub Actions run `32936194406` at `a4b14499` passes the memory domain, managed-service, campaign, and source-guard steps on macOS, Ubuntu, and Windows. Concurrent hostile filesystem path replacement remains outside the claimed trust boundary; static traversal and symlink escape fail closed.

### Out-of-process contributions

Native/OCI extensions, MCP, HTTP/OpenAPI, and optional remote providers/tools negotiate versioned capabilities. Unsandboxed native, script, and MCP processes are trusted host-authority code. Least authority is claimed only when verified confinement blocks direct host access and privileged effects cross kernel brokers.

### Content and frontend contributions

Skills, prompts, personas, tones, workflows, and catalog data are content packs. TUI, ACP, Web, IPC, CLI, daemon ingress, and schedulers are frontend/host adapters. Neither content nor frontends own runtime authority.

## Maintenance artifact

Slice 0 adds workspace package `omegon-maintain` at `core/crates/omegon-maintain/`, producing the separate `omegon-maintain` executable rather than a flag or second binary target in package `omegon`. Package `omegon-maintenance-contracts` at `core/crates/omegon-maintenance-contracts/` supplies only versioned deny, session-deny, ownership-record, exclusion-lock, transaction, audit, package-manifest, and canonical-key schemas used by both executables; it does not pull normal runtime code into maintenance. The executable shares the workspace release version, must not depend on package `omegon`, and is packaged beside `omegon` as a required release companion. It excludes normal TUI startup, the default agent loop, provider clients, project configuration evaluation, project contribution or extension code, MCP, mutable packs, memory, lifecycle, and orchestration.

The maintenance artifact supports:

- compiled maintenance-profile identity, exclusions, and bounded inert-entry diagnostics;
- inert contribution list/inspect plus maintenance-owned disable and reversible quarantine;
- session snapshot/metadata framing inspection plus resume-deny quarantine without semantic rewriting;
- durable ownership-record inspection and stale-record pruning without killing arbitrary processes;
- offline signed release archive and companion-artifact verification;
- maintenance audit inspection and verification.

Slice zero explicitly excludes generic read/search/patch/shell, project source or configuration mutation, semantic session repair, contribution enable/purge/install/update, process killing outside the current invocation, network release discovery/download, installation activation, update, and rollback. Later slices establish some prerequisites but do not implicitly add those maintenance operations; each requires separate requirements, safety analysis, and tasks.

The exact command tree and safety requirements are normative in `specs/kernel-composition/maintenance.md`; wire schemas, roots, locks, crash transitions, evidence rules, and command outcomes are normative in `specs/kernel-composition/maintenance-protocol-v1.md`. `docs/omegon-maintain.md` is their durable architecture summary.

Offline release verification has an explicit implementation prerequisite: the
selected verifier must support Sigstore bundle v0.3 and actually validate the
Fulcio chain, Rekor SET, Merkle inclusion proof, and signed checkpoint entirely
offline against compiled trust roots. The existing locked verifier supports only
older bundle profiles and omits those transparency checks, so mutation/audit
work lands as task 0.6a while verifier selection, trust material, and the signed
fixture matrix remain fail-closed task 0.6b. This split does not relax the
normative `release verify` contract.

Source, linked-development, and release-package launch paths are tested in this first slice rather than deferred to final packaging.

## Durable session authority

Before the shared supervisor becomes authoritative, define minimum append-only facts for:

- session creation and identity;
- prompt admission or rejection;
- queue insertion, removal, and ordering;
- turn start and generation identity;
- cancellation and revocation requests;
- invocation preparation and terminal settlement;
- turn completion, failure, cancellation, timeout, or unknown outcome.

The approved v1 vocabulary is `session.created`, `prompt.admitted`,
`prompt.rejected`, `prompt.removed`, `turn.started`,
`turn.interruption_requested`, `invocation.registered`,
`invocation.classified_unknown`, `invocation.settled`, and `turn.closed`.
Prompt admission and FIFO insertion are one transition; turn start atomically
promotes the queue head. Each event has a contiguous session-wide sequence and
immutable compatibility version. A snapshot records its stream, reducer, last
sequence, and last event identity and can reconstruct queue, active-turn,
cancellation, invocation, and terminal state without broadcast replay.

The append-only authority sidecar is distinct from current whole-file
conversation snapshots, metadata checkpoints, journals, and audit streams.
Legacy projections are not converted into fictional historical facts. Strict
replay rejects gaps, conflicts, invalid transitions, and unsupported authority
events or versions. The full envelope, reducer, recovery, and compatibility
contract is recorded in `docs/runtime-session-semantic-protocol.md`.

One supervisor implementation is instantiated per session. Cross-session hosts may own many supervisors, but TUI, ACP, daemon, Web/IPC, and bounded ingress submit to the owning session instance. Frontend busy and streaming state is a projection of supervisor state.

The default loop is a policy driver. It proposes typed step, message, invocation, continuation, and terminal intents. The kernel session state machine validates and commits each transition exactly once. The loop does not independently mutate canonical session truth or publish terminal completion.

Slice 1 registers invocation identity and conservatively classifies registered
unsettled invocations as unknown after runtime loss. Slice 3 owns durable
prepared/dispatched/acknowledged lease states and safe late settlement. Slice 5
adds complete model-context, route, assistant, tool, step, and compaction facts;
neither later slice redefines Slice-1 sequencing or closure semantics.

## Contribution lifecycle

### Static contributions

Statically linked contributions publish declarations before activation. The candidate graph validates IDs, invocation bindings, owner tier, dependencies, conflicts, platform support, effects, protocol ranges, and transition policy. Duplicate or ambiguous ownership is fatal unless an explicit replacement declaration names the superseded owner.

### Dynamic contributions

Dynamic protocols use two admission stages:

1. **Trust admission:** static preflight identifies code, protocol range, minimum dependencies, requested trust, and probe requirements. Probe code may execute only after explicit trusted-code admission or verified confinement.
2. **Capability admission:** the host starts an admitted probe in quarantine without brokered host-effect leases, negotiates a frozen declaration set, validates the graph, waits for readiness, and atomically promotes the candidate generation.

Heartbeat loss, startup timeout, crash loops, dependency degradation, restart/backoff, drain deadlines, forced cleanup, and quarantine are typed lifecycle outcomes. Required dynamic contributions must expose minimum dependency and trust requirements statically.

### Generations

Slice 2 binds registrations and candidate resources to one immutable composition generation. Candidate failure leaves the previous generation callable and removes every candidate registration/resource from publication; settled resources are released, while unresolved cleanup controllers remain under their rejected generation owner for retry. A graph-derived compatibility adapter feeds the legacy EventBus path, so registration order cannot select an owner rejected by the graph. Generation-bound invocation leases, stale-call denial, and privileged dispatch migration remain Slice 3. Model-visible schemas change only at turn-safe promotion boundaries.

The composition generation is distinct from a process or agent instance ID.
New sessions capture the active composition generation. Existing Slice-1 values
remain valid opaque legacy generation identifiers. Slice 2 does not add live
session migration; that requires a separately specified durable quiescent
migration event.

### Slice 6.3 dynamic contribution cutover

Slice 6.3 introduces one `DiscoveredContributionCandidate` inventory for native extensions, MCP process and HTTP servers, and executable manifest script, HTTP, and OCI contributions. A candidate contains only static identity, source digest, source kind, trust and confinement requests, probe requirements, and a transport adapter. Constructing the inventory cannot evaluate Pkl, spawn a process or container, connect to HTTP, resolve secrets, or register a feature.

One lifecycle pipeline admits each candidate against the captured digest, starts its transport adapter under one readiness deadline, freezes the adapter's declarations and compatibility payload, and stages all accepted features for the existing candidate graph. The EventBus graph publication remains the registration linearization point. Publication failure leaves no candidate registration visible and sends every probed resource to the same generation rollback owner. Successful publication transfers those resources to one dynamic generation owner, which also records restart/quarantine state, rejects stale generations, and performs normal shutdown once.

Transport adapters remain responsible for JSON-RPC and MCP framing, MCP resources and prompts, extension widgets and RPC handles, HostAction narrowing, secret delivery, process-group or container confinement, and HTTP remote-boundary semantics. Process-backed adapters must terminate and join their complete owned tree. HTTP adapters cannot claim remote peer settlement. Separate extension and MCP supervisor collections cease to be lifecycle authorities after cutover. Content packs remain Slice 6.4 and release composition remains Slice 7.

### Slice 6.4 content-pack protocol freeze

The shipped content artifact is `omegon-shipped` version `1.0.0` under content protocol 1. Its root contains `content-pack.toml` and only manifest-inventoried payload paths. The canonical digest is SHA-256 over the domain `omegon-content-pack-v1\0`, followed by each asset in ascending UTF-8 path order as an unsigned 64-bit big-endian path length, path bytes, unsigned 64-bit big-endian byte length, and the 32 decoded bytes of the asset SHA-256. The manifest itself is excluded from the digest. Every asset also carries its own byte length, digest, content kind, and requested content capabilities.

Admission requires schema version 1, a semantic pack version, complete publisher/source/revision provenance, a protocol range containing host content protocol 1, confined regular files, exact per-file and canonical digests, unique paths, and content-only requested capabilities. The only version-1 requests are metadata/template/directive/data requests for skills, prompts, personas, tones, workflows, and catalog data. These requests grant no prompt admission, tool callability, host effect, trusted path, executable trust, or persistent permission. Existing prompt safety, skill disclosure, persona activation, workflow selection, invocation, and dynamic-code admission remain separate gates.

The six axioms in `data/lex-imperialis.md` are constitutional host protocol, not shipped content: they define non-overridable epistemic and operator-agency constraints and remain kernel-resident. That file contains no runtime tool inventory or replaceable operational guidance. Capability guidance, tool recommendations, extension-specific contexts, and the session-compaction instruction are prompt assets in `omegon-shipped`; missing pack state omits optional augmentation and disables compaction locally before provider dispatch while retaining the six host axioms.

The process admits one immutable boot content generation named `content:<id>@<version>:<digest>` on first use. All sessions in that process retain that snapshot. A compatible pack replacement is admitted only on the next process boot; there is no mid-session migration. This explicit boot-only policy prevents prompt and tool-schema drift during active turns while permitting a v1-to-v2 pack replacement without rebuilding the executable. Missing, corrupt, or incompatible packs produce local unavailable diagnostics and do not block the constitutional kernel or maintenance executable.

Precedence is unchanged and explicit: project-owned content overrides user-owned content, which overrides extension content where supported, which overrides the shipped pack. Pack installation never copies authority or trust grants into operator settings. Packaging places the same self-contained pack under `share/omegon/content-packs/omegon-shipped` for linked development, direct installs, platform archives, Homebrew, Nix, and OCI; source execution uses the repository pack root. Standalone persona, tone, and workflow inventories are valid empty categories in version 1, while the loader and capability protocol reserve and test those categories without inventing content bodies.

### Slice 2 implementation boundary

Slice 2 owns composition discovery, pure graph validation, activation
eligibility, candidate generation construction, readiness, atomic publication,
health, drain, retirement, and shared diagnostics. It does not own privileged
invocation leases or replace the legacy dispatch engine.

Concrete ownership for Slice 2 is:

- renderer-neutral declaration and diagnostic contracts in `omegon-traits`;
- the pure graph builder and generation/lifecycle owner in
  `core/crates/omegon/src/contribution_graph.rs`;
- phased discovery and activation orchestration in `setup.rs`;
- a one-way graph-to-legacy compatibility adapter in `bus.rs`;
- transport-specific static preflight and quarantine adapters in existing
  extension and plugin/MCP owners;
- one semantic diagnostic projection under `surfaces/` for supported clients.

The first red-test matrix must prove deterministic all-error diagnostics for
duplicates, ambiguous bindings, cycles, missing owners, conflicts, and protocol
incompatibility; no dynamic spawn before trust/confinement admission; no
candidate registration publication before readiness; previous-generation
survival after candidate failure; no registration-order owner selection; no
active-session generation drift; and honest cleanup state across unowned
boundaries.

## Admission and invocation

The kernel invocation path covers model tools, operator actions, trust-boundary calls, calls consuming caller authority, durable mutations, and host-effect-bearing internal calls. Pure in-process computation and read-only domain queries may use typed service handles directly.

The path is:

```text
resolve owner and generation
  -> combine admission policy
  -> persist Prepared and issue lease
  -> persist Dispatched
  -> hand request to owner with stable call/deduplication ID
  -> persist Acknowledged when authoritative acknowledgement arrives
  -> persist Settled or Unknown
  -> close lease exactly once
```

`Dispatched` is durable before transport handoff. Every unsettled `Dispatched` invocation recovers as unknown completion. Mutating calls are not retried unless the owner contract enforces idempotency or deduplication for the stable call ID. Failure to persist settlement fences further mutation and records emergency recovery evidence; it does not report ordinary completion.

Admission is monotonic. Policy sources can narrow authority, but no downstream adapter can widen a denial. Unknown owners, capabilities, effects, schemas, or provenance receive no privileged lease. Declarations request effects; they do not prove confinement.

Slice 3.1 establishes the authoritative in-memory seam for model-tool calls: accepted-graph resolution, caller/surface scope, the current name-based RBAC ceiling, layered permission policy, operator approval, lease issuance, immediate generation/owner revalidation, owner handoff, and exactly-once in-memory closure. The lease captures composition and owner generations, capability, principal, call, scope, admitted declaration effects, and transition policy. Unknown or incompletely declared invocations receive no lease. A publication that changes the composition generation makes an undispatched lease stale; rejected candidates preserve the prior generation and do not revoke it.

This first lane does not claim crash consistency. Slice 3.2 replaces compatibility name-derived RBAC, effects, timeout, parallelism, retry, and transaction metadata with declaration authority for leased model-tool calls. Capability declarations now carry typed principal eligibility, bounded timeout class, retry/idempotency/deduplication policy, serial or parallel-safe scheduling, and explicit independent-mutation or best-effort-rollback behavior. Validation fails closed on missing principals, zero attempts, unsafe non-idempotent retry, parallel rollback, and effect/transaction contradictions. The lease captures and dispatch revalidates the complete policy; RBAC derives from declared effects, caller timeout arguments can only narrow the declared class ceiling, and the scheduler no longer uses a tool-name parallel allowlist or behavioral mutation catalog as authority.

Static feature and legacy provider hooks may publish precise tool policy; absent hooks receive a conservative host-authored adapter, and external tools retain the full host-effect envelope rather than fabricated least authority. Retry metadata is captured but does not authorize crash or unknown-completion replay.

Slice 3.3 persists `invocation.prepared` after admission and approval but before materializing an executable lease, then persists `invocation.dispatched` after exactly-once lease claim and generation revalidation but before host or local owner handoff. Preparation captures lease and invocation IDs, provider-visible call ID, optional owner-enforced deduplication ID, turn, principal, capability and owner generations, effects, execution policy, transition policy, and surfaces. The same invocation metadata reaches local provider, extension, MCP, and host-delegation adapters; unsupported transports do not claim deduplication. Interactive, ACP, daemon, and bounded turns share the session authority writer and exact durable turn scope, while explicit no-session compatibility calls remain ephemeral. Durable edit calls execute one visible call per owner handoff instead of collapsing several call identities into a hidden batch.

`Prepared` and `Dispatched` are distinct from legacy `invocation.registered`, whose conservative recovery behavior is preserved for old streams. Slice 3.3 recovery retains new prepared and dispatched states without inventing acknowledgement, settlement, or unknown completion; those transitions begin in Slice 3.4. A failed preparation writes no lease, and a failed dispatch append revokes the claimed lease before owner entry. The append-only JSONL fact remains authoritative if the replaceable snapshot cache update fails. Existing direct EventBus/internal/operator paths remain named compatibility paths until Slice 3.7; they are not silently assigned fabricated principals or administrator leases. Reactive path grants and extension HostAction approvals remain narrower post-lease guards and cannot override an upstream denial.

Slice 3.4 persists owner acknowledgement and typed terminal settlement before ordinary completion or lease closure. Unsettled dispatched and acknowledged calls recover as unknown completion, while prepared calls remain explicitly unhanded-off. Live external transport ambiguity uses the same unknown classification and is not automatically replayed.

Slice 3.5 requires every mutating execution declaration to carry a validated durable mutation domain and fence key. A post-dispatch acknowledgement, unknown-classification, or settlement durability failure writes strict append-only emergency evidence through a directory shared by session authorities and independent of the failed authority JSONL. Evidence binds call, capability, owner and composition generations, invocation, lease, session, turn, fence, and failure phase. Matching mutation is denied immediately before preparation; malformed evidence and emergency-writer failure fail closed. No ordinary runtime path removes a fence, and the current maintenance companion exposes no clearing command without a later deterministic reconciliation or explicit audited recovery contract.

Slice 3.6 checks stable call identity against unknown invocations across the complete session before preparing another lease. A mutating unknown is unsafe unless its original persisted execution contract was idempotent or carried owner-enforced deduplication for that exact call ID; a current replacement declaration cannot retroactively make the prior handoff safe, and legacy unknown records fail closed. Unsafe replay receives a typed denial before another `Prepared` fact. Ambiguous ACP host-write responses no longer trigger a second local write. This slice does not enable safe replay, relax duplicate-call invariants, or introduce attempt lineage/request fingerprints; provider-request retries remain separate because they precede completed tool-call dispatch.

Slice 3.7 makes admission and dispatch kind-aware rather than tool-only. Graph-registered feature commands invoked through TUI, CLI remote execution, ACP, Web, or IPC carry explicit operator principals and owner-declared surfaces through generation revalidation, acknowledgement, settlement, and exactly-once lease closure. Model-loop path grants invoke the graph-declared `trust_directory` internal owner under an internal principal while retaining the parent session and turn authority. Automatic memory ingestion and host-mediated persona/tone switches use explicit internal bindings and leases; model-facing memory mutations declare state-changing effects instead of read-only orientation. Managed-delegation tools declare both model and service principals plus Model/Web/Daemon surfaces, and authenticated supervisor calls use service leases instead of direct EventBus dispatch; `agents_status` resolves to the owned delegate-status tool rather than an unowned name. Operator context-pack requests call the shared context provider through a typed read-only service handle rather than impersonating a tool invocation. The extension-provided `voice_session_stop` tool declares model and TUI surfaces plus service authority, and the interactive coordinator invokes it under the promoted turn's real authority. Daemon vox polling invokes the declared `vox_route` tool under an ephemeral Service/Daemon lease and projects its result into the existing daemon event envelope. Arbitrary ACP methods share one extension-owned conservative transport capability because the current protocol does not declare per-method effects; the ACP thread tracks availability, while the worker-owned EventBus admits, revalidates, acknowledges, dispatches, settles, and classifies ambiguous transport loss. Lease-less imperative `actions/execute` requests fail closed before host execution, and an operator approval only contributes the operator gate instead of fabricating project, runtime, or trusted-origin authority. Declarative native HostActions and MCP review candidates receive a host-only parent guard after lease revalidation; it requires the parent to remain dispatching, verifies conservative HostAction effects are contained by the parent declaration, and consumes each child dispatch identity once. These migrations do not fabricate durability for idle or post-loop calls: without an active authority turn they remain explicit ephemeral leases.

## Process ownership

Every process, task, socket, listener, subscription, temporary file, and durable writer has one host-recorded owner and generation. Complete tree settlement is required only inside a lifecycle boundary Omegon can own. Cross-boundary processes, including Windows-host executables launched from WSL, settle as degraded or unverified; profiles requiring strict cleanup reject those transports.

## Semantic event spine

The minimum supervisor facts expand into a complete semantic session event contract containing admitted input, model-visible context provenance, provider route and schema generation, assistant output, tool calls/results, invocation states, step/turn boundaries, and cancellation/interruption evidence.

Slice 5.1 uses one durable step for each internal loop iteration. A step may contain
multiple ordered model requests only when the same iteration repairs
context-overflow or provider-history state; every repaired request receives a new
request identity and route lease while retaining the step identity. The existing
`route.lease_recorded` v1 payload remains immutable. A separate
`model.request_route_joined` fact binds that lease to the request and step.

Model-visible context is recorded as an ordered manifest of immutable content
references and provenance. The tool schema set has a content-addressed identity
over its canonical ordered composition plus the composition and owner generations
that produced it. Assistant display content is durably appended as ordered,
bounded, coalesced chunks before broadcast; durability is per bounded append, not
per provider token. Hidden reasoning and opaque provider continuity are persisted
only when a provider requires them for continuation, in restricted
content-addressed blobs excluded from default projections. A captured
generation-bound provider policy is either `none` or `restricted_required` with
declared kinds and a size ceiling; it does not widen `route.lease_recorded` v1.
Arbitrary raw provider payloads are never authority facts or continuity blobs.
Transport retries under one joined request carry contiguous response-attempt
ordinals on chunks, continuity, message commit, and request closure. Failed
attempt chunks remain canonical but cannot enter another attempt's commit; each
failed attempt is durably identified and terminalized before a later attempt.

Slice 5.1 emits these facts only for authority-backed sessions. It makes denied
tool calls and their denied results canonical, links admitted calls and terminal
results to existing invocation facts, and adds deterministic request/step
abandonment sufficient to recover a crashed writer. It does not add tool-progress
facts. Complete replay policy, consumer migration, compaction derivation,
sessionless semantic streams, and lagged/corrupt consumer fixtures remain tasks
5.2 through 5.5.

Current storage is plural input to migration:

- resumable whole-file snapshots persist an LLM-facing projection;
- checkpoint JSONL persists metadata rather than semantic replay facts;
- the agent journal is a human narrative;
- audit streams have separate schemas and authority.

Storage backends, provider-history projection, transcript rendering, compaction, and snapshots derive from the semantic record. No current stream is silently reclassified as canonical without explicit conversion and compatibility tests.

## Migration strategy

1. Build and release-test the independent maintenance artifact.
2. Define minimum durable session facts and promote one supervisor per session.
3. Evolve authority-neutral capability declarations into the pre-activation contribution graph.
4. Extract admission, leases, scheduling, progress, terminalization, and retry classification into one invocation pipeline.
5. Unify provider contribution metadata and reduce the loop to a transition-intent driver.
6. Complete the semantic event spine and migrate projections/storage.
7. Extract optional domains and unify external contribution discovery.
8. Complete artifact separation, contribution locks, and composition budgets.

Each step retains adapters for current behavior until parity and absence tests pass. Crate movement without authority, lifecycle, or failure-isolation change does not satisfy a migration task.

## Documentation co-delivery

Documentation is part of each implementation lane, not a final release activity. Before mutation, every lane classifies:

- the durable architecture/developer documents it owns;
- serialized compatibility or migration notes;
- operator-visible command, configuration, output, permission, recovery, packaging, or availability changes;
- affected public pages under `site/src/pages/docs/`;
- affected canonical command examples under `site/snippets/`;
- the narrowest documentation and site validation commands.

A lane cannot pass its exit gate until implemented behavior, source design, OpenSpec requirements/tasks, durable docs, public site pages, snippets, CLI help, and operator terminology agree. A lane with no public delta records that decision explicitly. Later groups may refine earlier documentation when authority migrates, but they may not serve as a deferred documentation bucket for behavior already shipped.

## Safety invariants

- Trust admission precedes execution of dynamic probe code.
- Requested confinement fails closed when the required boundary is unavailable.
- Unknown effects and capability owners fail closed.
- Mutating unknown-completion calls do not retry without owner-enforced deduplication.
- Frontends can expose less authority than the runtime, never more.
- No optional contribution or normal integration startup path is required to diagnose, deny, or quarantine that contribution.
- Durable facts and reconstructable snapshots precede advisory broadcasts.
- Strict cleanup claims are limited to process trees inside the host ownership boundary.
- Documentation and applicable public site/snippet changes pass within the same lane as their behavior.

## Slice-1 design gate

The minimum semantic vocabulary, serialized evolution policy, strict replay
rules, snapshot reconstruction contract, deterministic interruption closure,
and Slice 1/3/5 ownership split are approved in
`docs/runtime-session-semantic-protocol.md` and the runtime-session authority
spec. Implementation proceeds with an adjacent authority sidecar and must not
weaken the stronger stale-interrupt and exactly-once settlement behavior in the
currently compiled interactive coordinator.

## Slice-4 design gate

Slice 4 separates provider declaration from request routing. Task 4.1 adds and
validates provider contributions; it does not claim one cross-host route
authority or durable request leases. Task 4.2 consumes those contributions in a
single route service and records the minimum route lease before provider
dispatch. Slice 5 later extends the semantic event spine and its projections; it
does not postpone the route identity required to explain a Slice-4 dispatch.

Provider contribution metadata is owned in the `omegon` integration crate so a
bridge-factory binding can resolve the crate-local `LlmBridge` contract without
pulling provider transports or secrets into `omegon-traits`. Existing
`RuntimeContributionId` and generation IDs remain the owner vocabulary. The
runtime inference inventory remains the authority for layered endpoint,
offering, modality, capability, and provenance evidence; a provider contribution
binds to that authority rather than duplicating model rows. Executable factory
bindings are host-local typed identities, not serialized code pointers.

Provider identity, endpoint/deployment identity, model offering, conceptual
model identity, and credential source remain distinct. Authentication class
describes accepted credential mechanisms, not merely login UI. Tool-schema
dialect describes the executable adapter behavior and may explicitly declare
tools unsupported. Fallback compatibility is directed, model-family-bounded,
and non-transitive. OpenAI-compatible wire support alone never establishes
fallback eligibility; current route policy and admission may only narrow a
declared relation.

Compatibility adapters may retain current provider constructors and host-local
route callers through task 4.1, but every executable provider must resolve to
one complete contribution independent of registration order. Task 4.2 removes
host-local bridge/fallback construction across interactive, daemon, ACP, child,
and bounded execution. Selecting another route between requests is distinct
from replacing the route-service implementation; the latter remains boot- or
quiescent-boundary only.

The task-4.1 red-test matrix covers duplicate provider IDs and aliases, missing
inventory/auth/schema/factory/evidence semantics, unsupported tool contracts,
dangling or wire-only fallback relations, stable ordering, and parity between
legacy constructible providers and the validated registry. OpenAI/Codex and
Google/Antigravity relations remain explicit and model-bounded; unrelated
providers remain isolated. Task 4.2 adds cross-host route parity, selected versus
serving identity, stale generation, and pre-dispatch durability tests.

Task 4.1 updates the owning provider/decomposition developer docs and release
notes. Public model-selection, authentication, fallback, and route-evidence
guidance changes only when operator-visible routing consumes the declarations;
if command syntax remains unchanged, canonical snippets remain unchanged and
that decision is recorded in the task.

Task 4.2 implements the request boundary in `ProviderRouteService`. Existing
provider constructors remain transport factories behind that service, while
compatibility `LlmBridge` handles carry immutable selected and serving route
hints instead of erasing fallback identity. The common loop dispatch path,
compaction, smoke execution, and lightweight completion record a lease before
calling `stream`; daemon, ACP, child, bounded, and interactive hosts inherit the
same path rather than constructing fallback chains.

Session authority accepts `route.lease_recorded` only for its active turn and
reduces leases by immutable lease identity. Sessionless inference writes the same
versioned lease payload to an explicit step-owned durable JSONL stream under the
runtime home. A partially populated session scope is not downgraded to a step.
Current contribution generation and directed fallback compatibility are
revalidated immediately before persistence, and persistence failure blocks the
provider call. Full context, assistant stream/result, and projection events
remain Slice 5 work rather than being folded into this minimum dispatch fact.

Task 4.3 introduces a single compiled `ReleaseCoupledLoopDriver` with four
required trait contracts: session, leased route, context assembly, and privileged
invocation. Compatibility adapters remain crate-local so this boundary does not
move provider transports, mutable conversation state, feature composition, or
context implementation into shared protocol crates. Every compiled host uses the
same turn constructor; no optional port or raw-loop host entry point remains.

The session contract captures the invocation principal, session/turn authority,
and ephemeral route-step identity. Driver admission rejects partially populated
authority, authority from another session, and a turn other than the authority's
active turn. The route contract captures the controller and bridge together and
requires coherent serving identity; only an explicitly disconnected bridge may
omit it. The loop implementation consumes those captured contracts rather than
reading route or invocation authority directly from policy configuration.

Normal driver termination produces one typed terminal proposal for completed,
provider-exhausted, or failed execution. Interactive, ACP, daemon, and bounded
hosts submit that proposal through the existing supervisor only after their
owned cleanup, preserving cancellation/timeout narrowing and canonical
authority ordering. Advisory completion broadcasts remain projections. Task 4.4
removes concrete implementation and feature names from policy behind these
contracts; task 4.5 governs replacement at boot or quiescence; Slice 5 adds the
message, context, assistant, tool-result, continuation, and step facts that do
not yet exist in the minimum authority vocabulary.

Task 4.4 keeps the compatibility implementations release-coupled but removes
their concrete names and authority inputs from production loop policy. The
driver captures one opaque host binding and constructs the four required ports;
provider route policy is snapshotted by the route port instead of calling back
into `LoopConfig`. Tool admission, batching, owner handoff, permission and
operator-wait presentation, memory/lifecycle requests, context implementation,
and plan/recovery/finalization policy remain behind their owning contracts.
Source guards cover concrete provider and transport names, direct invocation
lifecycle calls, tool-name batching policy, memory attribution, concrete context
implementation, frontend permission presentation, lifecycle finalization, plan
and completion policy, and adapter callbacks into loop orchestration. This does
not make the driver dynamically replaceable, establish a quiescent replacement
boundary, or add complete semantic step/message persistence.

Task 4.5 adds one session-lifetime execution owner around a selectable atomic
pair of loop-driver and provider-route-service handles and their validated
contribution-generation IDs. Durable turn start and pair capture share the
owner's coordination gate, so a turn receives either the complete prior pair or
the complete migrated pair. Sessionless execution retains the immutable boot
pair and cannot request migration.

A mid-turn replacement request stores only in-memory `Pending` intent and leaves
the active capture unchanged. Turn closure does not promote it, and starting a
later turn does not promote it. A deliberate caller must invoke
`commit_pending_at_quiescence`; only that operation may append the version-1
`session.execution_binding_migrated` fact and then publish the target pair.
Admission requires an idle session, the exact process-local and durable source,
and no registered, prepared, dispatched, acknowledged, legacy-unknown, or
durable-unknown invocation. The append is synced before in-memory publication,
and failure retains both the current pair and pending intent.

The reduced binding remains optional so legacy streams replay without invented
history. Resume establishes the selected boot binding process-locally and
appends nothing; it must match durable migration history when such history
exists. Interactive, ACP, daemon, bounded, headless/child, and Sentry execution
now use an owner capture, while smoke, compaction, and lightweight sessionless
routes use the immutable boot binding. Source guards reject direct driver/route
construction and supervisor auto-commit. This does not add Slice 5 message,
context, tool-result, continuation, or step facts; task 4.6 closes the separate
documentation lane below.

Task 4.6 closes Slice 4 by reconciling the canonical provider-contribution and
route-lease guide, related durable routing/session/recovery documents, public
model-selection, fallback, authentication, and route-evidence guidance, the
README, and release notes. Current documentation does not turn declaration or
inventory presence into execution, claim global failover, treat compatibility
as symmetric or transitive, present Antigravity as executable, claim Anthropic
subscription headless use is blocked, expose historical leases through
`/model`, make inventory diagnostics a dispatch gate, promise an exhaustive
current provider/model inventory, or infer a precise credential store from
authentication-class-only evidence. Command and configuration syntax did not
change, and canonical site snippets remain unchanged. Slice 5 semantic event
spine work is still explicitly outside this closeout.

## Slice-5 design gate

Task 5.0 freezes the production-emission contract in
`docs/runtime-session-semantic-protocol.md`. The new required v1 vocabulary is
`step.started`, `model.request_prepared`, `model.request_route_joined`,
`assistant.content_appended`, `assistant.message_committed`,
`provider.continuity_stored`, `tool.call_recorded`, `tool.result_recorded`,
`model.request_closed`, `step.closed`, and `step.abandoned`. Slice-1 envelope,
sequence, command idempotency, and `turn.closed` semantics remain unchanged, as
does `route.lease_recorded` v1.

One `loop.rs` internal iteration maps to exactly one step. Step ordinals are
contiguous within a turn. Request ordinals are contiguous within a step and may
advance without closing the step only for context-overflow/history repair. Each
request has exactly one prepared fact, one route join to a distinct route lease,
and one terminal request outcome. Context items, assistant chunks, tool calls,
and tool results use contiguous zero-based ordinals in their declared scope.
Normal step closure requires every request and canonical tool call terminal.
Recovery appends deterministic open-invocation classifications first, then
open-request abandonment, then `step.abandoned`, then the existing
`turn.closed(interrupted)`. Live abnormal hosts use the same ordering after owned
cleanup, with the strongest truthful request outcome and the existing narrowed
failed, timed-out, cancelled, revoked, or interrupted turn outcome.

Content bytes referenced by Slice-5.1 facts are stored in a session-adjacent,
content-addressed blob store rather than embedded in authority records. References bind digest algorithm/digest, media
type, byte length, storage class, and projection class. Reads are descriptor-
confined to the session store and verify all reference fields before decode.
Default projections may read ordinary display/model content but never restricted
continuity blobs. Restricted blobs require the captured provider-continuation
owner and exact session/request purpose; exports, transcripts, diagnostics,
memory, and arbitrary extensions receive no implicit access.

Task 5.1 owns only authority-backed production emission and the minimum
deterministic abandonment path necessary for crash safety. Red tests must cover
multi-request same-step repair, lease/request mismatch, noncanonical schema-set
identity, chunk coalescing and ordinal gaps, projection before append, restricted
blob leakage or digest substitution, denied calls, invocation mismatch,
duplicate terminal facts, EOF/cancellation outcomes, and repeated recovery.
Tasks 5.2 through 5.5 own complete compatibility/replay matrices, reducer-backed
provider history and transcripts, legacy consumer/storage migration, compaction
checkpoints, and lagged/disconnected/corrupt consumer recovery. Sessionless full
semantic streams require a separate future design. Task 5.6 completed final developer and applicable public recovery
documentation. Task 5.0 itself changes no command, configuration, site page,
canonical snippet, or runtime behavior, so it requires no Unreleased behavior
entry.

Task 5.1 is production-active for complete authority-backed scopes. Normal loop
paths emit and terminalize one step exactly once. Runtime loss and host abnormal
paths classify unresolved handed-off invocations, terminalize any open request
with its latest response-attempt identity, append deterministic reason-bound
UUIDv5 `step.abandoned`, and only then permit canonical turn closure after host
cleanup. Interactive cancellation cooperatively drains the same pinned execution
under the supervisor deadline; timeout abort drops its state/authority borrows
and joins its turn-scoped projection relay before abandonment. Detached native
HTTP transport may finish externally but has no remaining receiver or projection
capability, so that completion is explicitly unverified rather than falsely
settled. Partial scopes fail closed and sessionless execution emits no fabricated
semantic spine. This does not claim task-5.2 replay rules, task-5.3 projections,
task-5.4 storage migration, or task-5.5 consumer recovery.

Task 5.2.0 freezes those next contracts without changing runtime. Once a lineage
contains its first full-spine fact, every later eligible operation must use the
full spine; there is no downgrade, concurrent old writer, or negotiated minimum
reader level. Legacy streams remain legacy, mixed streams have an explicit
boundary and exact full-spine suffix, and full streams use semantic authority
from their first eligible operation. Older or incomplete readers fail closed.
Missing or tampered referenced blobs prevent full recovery; only diagnostic
projections may render an unavailable marker, never provider history or exact
exports. Legacy transcript bytes are not promoted into authority.

The refinement adds new required v1 names without changing any existing v1
payload: `context.source_materialized`, `model.response_attempt_failed`,
`compaction.started`, `compaction.request_prepared`,
`compaction.response_attempt_failed`, `compaction.request_closed`,
`compaction.summary_committed`, `compaction.applied`, and
`compaction.abandoned`. Turn-owned compaction binds an
open turn/step and the unchanged route lease. Manual idle compaction instead
uses a session-scoped compaction identity and embedded route evidence; it does
not invent a prompt, turn, step, or `route.lease_recorded` payload. Idle
compaction holds the supervisor admission gate and publishes one atomic context
replacement only after its applied fact is durable. Sessionless work remains
route-only.

Task 5.2 implements strict event/reducer compatibility, response-attempt
validation, read-only prefix replay, reducer/cache v5, and the generic projector
cursor v1 contract. It does not switch provider history, transcripts, frontend
snapshots, or compaction checkpoints to those reducers. It also emits and reduces
the frozen provenance and compaction facts, recovers compaction before ordinary
turn terminalization, and atomically replaces idle host sessions only after
strict target validation. Task 5.3 derives the concrete projections with
output-before-cursor publication; it does not re-emit semantic authority. Task
5.4 now migrates the frozen storage and consumer set while preserving plural
authority and exact-frontier rules; sessionless semantic lineage remains
deferred rather than synthesizing history. Task 5.5 exercises
the frozen canonical corpus under lag, restart, disconnect, truncation,
corruption, and blob-loss conditions. Task 5.6 completes compatibility
publication and applicable public/developer documentation closeout.

Task 5.3.0 freezes the concrete derivation boundary before task 5.3 mutates
runtime projection code. Four internal semantic projectors, all projector
version 1 and schema version 1, are fixed as `session.provider-history`,
`session.transcript`, `session.frontend-snapshot`, and
`session.compaction-checkpoint`. Provider history is an immutable sequence of
exact joined-request inputs and can never be treated as synthesized input for a
later request. The normal transcript contains committed prompt, assistant, and
tool-result messages only. A frontend evidence snapshot may additionally show
durable uncommitted or abandoned assistant chunks, queue, active-turn, context,
and semantic-conversation state; downstream live tool progress remains an
ephemeral overlay rather than durable projection content. Committed content
followed by abandonment remains visible and carries abnormal status.

Every output uses the frozen availability envelope. A full lineage may claim
`exact_full`; a mixed lineage may claim only `exact_suffix` beginning at its
first full-spine fact and must mark full-session export unavailable; a legacy
lineage publishes an availability envelope with no content or exactness claim.
Restricted continuity is metadata-only where needed to prove request input and
is never dereferenced or serialized. Provider-history and transcript bodies use
immutable bounded chunks plus a bounded manifest that remains below the generic
16 MiB cursor-output ceiling. Frontend and compaction checkpoint each publish
one bounded output. RFC 8785 canonical JSON, semantic-key ordering, SHA-256, and
source event frontiers make byte output independent of wakeup batching.

One session publication coordinator captures the latest stable read-only replay
frontier, coalesces redundant wakeups, runs all four projectors against that
same frontier, and independently performs chunk-before-manifest and
output-before-cursor publication. A failed projector retains its previous
committed frontier without blocking the other three; no partial or unavailable
content is mislabeled exact. Projector-owned chunks are immutable and retained
for the authority lineage lifetime, while unreferenced temporary files may be
removed only under the projector lock. Task 5.3 remains shadow-only: it creates
and verifies these internal outputs but does not switch `ConversationState`,
provider dispatch, transcript commands, TUI, ACP, Web, IPC, whole-file
snapshots, or compaction compatibility consumers. Those switches remain task
5.4. The refinement itself is planning-only and therefore has no public docs,
site, command/configuration/snippet, or changelog behavior impact.

Task 5.3 implements that boundary with one capacity-one, dirty-bit worker owned
by each authority-backed runtime supervisor. Durable appends signal only after
sync; ordinary bursts coalesce for 50 ms with a 250 ms ceiling, while frozen
terminal, explicit flush, startup/recovery, and shutdown boundaries publish
immediately. The worker strict-replays end-of-stream before each run and records
typed replay, coordinator, and per-projector failures without entering the
authority append path. Session replacement clears and stops the old worker,
fences its session-specific adjacent root, and transfers its join handle to the
new supervisor for owned reaping without delaying host publication. Sessionless supervisors create no worker. Task 5.4 has now replaced the shadow-only consumer boundary with validated readers and source guards against direct projection-storage access.

Task 5.4.0 froze the consumer migration before task 5.4 changed runtime. The
authority-role matrix is intentionally plural: the semantic stream owns replay
facts and exact committed transcript; a synchronous immutable current-context
reducer owns provider-dispatch input at the captured authority frontier;
versioned host-state checkpoints own `IntentDocument` and plan state; an
append-only observation ledger owns operator observations; operator metadata owns
friendly name and description; semantic projections derive counters, catalog,
telemetry, frontend evidence, and compaction state; audit and Markdown journal
remain separate diagnostic/narrative records. No projector, compatibility
snapshot, journal, audit row, or observation may be promoted into semantic
authority.

Provider dispatch never waits for or reads provider-history publication. Under
the session admission/writer coordination boundary it captures the latest
durable frontier and synchronously reduces bounded, provenance-complete model
context into `CurrentContextViewV1`; a gap, unsupported fact, missing blob,
unattributed post-boundary item, bound violation, or frontier mismatch fails
before dispatch. Exact resume uses the same law. Frontends may render a validated
older snapshot only with its disclosed cursor and lag; no stale snapshot can
authorize dispatch or an exact-resume claim.

Full lineages resume and export exactly from semantic state. Mixed lineages may
resume only as an explicitly labeled compatibility legacy base followed by the
exact semantic suffix; the base never acquires historical semantic authority and exact
full-session export stays unavailable. Web historical output for mixed lineages
is the exact suffix only. Legacy sessions retain labeled compatibility resume;
sessionless semantic lineage remains deferred. `/transcript` is reassigned to
the exact committed semantic transcript, while `/session-export` is the distinct
presentation/evidence export name.

The new host-state, observation, catalog, telemetry, audit-source, and journal-
provenance schemas start at version 1 without changing authority event v1,
projection schema v1, reducer/cache v5, or legacy file shapes. Slice 5.6 stops
rewriting `.json`/`.meta.json` for full and materialized-mixed sessions; legacy
and not-yet-materialized mixed sessions retain the pair only as a one-way import
source. The semantic writer and forward-only lineage remain active, and no old
writer may append a reduced event set. Task 5.4 implements the consumer cutover,
task 5.5 supplies adverse-consumer execution, and task 5.6 completes
compatibility publication plus applicable public documentation.

Task 5.5.0 freezes that adverse-consumer execution without changing production.
The private semantic protocol is the normative campaign: 54 stable scenario IDs
form a pairwise covering array across lineage (`legacy`, `mixed`, `full`),
lifecycle (late attach, lag, disconnect, restart, replacement, and steady
publication), and consumer class (exact, projection, frontend, host record,
evidence, and compatibility mirror). Faults and dispositions are closed typed
vocabularies, and each consumer class has one fail-closed, explicitly degraded,
or best-effort law. Tests use copied fixture sandboxes and deterministic
append/sync/rename/read, notification, worker, and replacement barriers rather
than sleeps or mutation of the checked corpus. Required CI runs on Linux,
macOS, and Windows and must complete within 15 seconds per platform; broader
filesystem crash probes remain non-blocking evidence.

The freeze resolves the remaining policy choices. A projector may quarantine a
proven corrupt chunk and deterministically republish it from validated authority
only while holding that projector's lock; no other store may be silently
repaired or quarantined. Session replacement validates authority, blobs,
host-state, observations, and catalog identity, but derived projection damage or
absence is disclosed and does not prevent publication while the replacement
worker rebuilds. ACP worker/supervisor completion and authoritative idle queue
state release local busy gates even when `AgentEnd` or equivalent advisory
notification is skipped; notification draining is bounded. IPC lag enqueues one
reconciled current state automatically before later deltas. A missing observation
ledger degrades open only when no durable marker, host frontier, catalog field,
or mirror provenance says it existed; malformed or torn bytes fail closed.
Malformed semantic audit input stops the semantic audit cursor and warns, with
no silently generated duplicate or quarantine row. Journal failure to read an
existing authority is `semantic_source_unavailable`, not sessionless. Existing
authority with no catalog record is a fatal store-set invariant. A durable
semantic save followed by mirror failure returns typed partial publication and
remains semantically resumable.

The current corpus has authority/reducer vectors but not the frozen cross-
consumer cases for corrupt projector chunks, damaged-projection replacement,
skipped ACP notifications, automatic IPC lag reconciliation, ledger existence
proof, malformed semantic audit input, journal authority unavailability,
missing catalog identity, partial mirror publication, or platform-specific
atomic publication. These are exact red gaps, not permission to revise accepted
vectors. Task 5.5 may correct behavior exposed by those fixtures while retaining
event v1, reducer/cache v5, cursor v1, projection v1, and task-5.4 store schemas.
Task 5.6 is the only compatibility-publication-removal and
developer/applicable-public-doc boundary. Its refinement defines semantic
self-sufficiency narrowly: full lineage, or mixed lineage carrying exactly one
durable content-addressed legacy compatibility base. Those sessions stop
rewriting `.json`/`.meta.json`; legacy and not-yet-materialized mixed sessions
retain the pair only as a one-way import source. Opening a valid legacy pair
beside pre-boundary authority materializes that base before the first full-spine
step and fixes mixed lineage. Existing pair artifacts are not automatically
deleted, but stale or missing pair bytes cannot affect a self-sufficient
session. Maintenance becomes catalog-first for inventory, inspection, and
quarantine, with pair fallback only for legacy import. Closeout does not invent
the previously described rollback consumer switch: no such runtime selector
exists. Event v1, reducer/cache v5, cursor v1, projection v1, and task-5.4 host
schemas remain unchanged. The refinement itself was planning-only; task 5.6 now
implements this boundary and its documentation without changing those frozen
schemas.
