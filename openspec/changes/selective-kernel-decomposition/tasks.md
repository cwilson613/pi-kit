# Selective Omegon kernel decomposition - Tasks

Dependencies: groups land in order `0 -> 1 -> 2 -> 3 -> 4 -> 5 -> 6 -> 7`. Subtasks within a group may overlap only after their shared contracts and red tests exist. Every group retains compatibility adapters until its exit gate passes.

Definition of done for every group: classify documentation impact before implementation; update the owning durable design/developer docs; update applicable `site/src/pages/docs/` pages and canonical `site/snippets/` command examples for operator-visible changes; record why no public change is needed when applicable; and run the narrowest relevant docs/site validation before the group exit gate.

## 0. Design baseline and independent maintenance artifact
<!-- specs: kernel-composition/documentation, kernel-composition/maintenance, kernel-composition/maintenance-protocol-v1 -->

- [x] 0.1 Record the comparative architecture assessment and selective decomposition source design.
- [x] 0.2 Draft the initial proposal, design, normative specs, and ordered implementation tasks.
- [x] 0.3 Specify the maintenance crate topology, command contract, shared exclusion/deny/transaction schemas, mutation roots, deadlines, output schema, dependency posture, and excluded/deferred operations.
- [x] 0.4 Build `omegon-maintenance-contracts` with canonical v1 fixtures and red interoperability, corruption, lock-race, crash-point, path, and outcome tests.
- [x] 0.5 Build the separately runnable maintenance artifact with compiled-profile composition, inert contribution, session-framing, and durable ownership-record diagnostics.
- [x] 0.6a Add inert contribution disable/quarantine, session quarantine, stale-record pruning, and maintenance audit workflows.
- [x] 0.6b Integrate vetted offline Sigstore bundle-v0.3 verification, compiled Fulcio/Rekor trust roots, signed-checkpoint/SET/inclusion-proof validation, and the release fixture matrix before enabling `release verify`.
- [x] 0.7 Integrate and test deny/exclusion locks in every normal contribution startup path, session-deny locks in every resume path, and v1 ownership-record writers.
- [x] 0.8 Package and launch-test source, linked-development, direct-install, platform archive, Homebrew, Nix, and OCI paths supported by the repository.
- [x] 0.9 Prove startup with TUI, default loop, project config/plugins, extension runtime, MCP, mutable packs, memory, lifecycle, and orchestration absent.
- [x] 0.10 Co-deliver maintenance architecture/operator docs, public install/recovery pages, canonical command snippets, and site validation with the artifact.
- [x] 0.11 Remove Homebrew publication as a Slice-zero exit gate. Existing formula verification remains a best-effort packaging safeguard, but publishing a qualifying Homebrew archive is not required to begin later slices. Documentation impact: internal lifecycle policy only; existing public Homebrew guidance and packaging behavior are unchanged.
- [x] 0.12 Make pairwise self-update crash-atomic and rollback-safe through immutable version-directory publication plus one atomic activation switch, with failure injection across activation, launcher, and receipt boundaries. Documentation impact: updated the durable installation/version-switcher contracts, public install guidance, and release notes for `versioned-current-v1`; no canonical command syntax changed.

## 1. Minimum durable session authority
<!-- specs: kernel-composition/documentation, runtime-session/authority -->

- [x] 1.1 Approve the minimum semantic event vocabulary, sequence/version rules, and compatibility policy.
- [x] 1.2 Add the approved adjacent authority stream, strict reducer/cache, and durable prompt, queue, turn, interruption, minimum invocation, recovery, and terminal facts.
- [x] 1.3 Refactor the existing supervisor scaffold into one frontend-neutral compiled implementation instantiated once per session. Documentation impact: internal architecture only; no public commands, site pages, or snippets changed.
- [x] 1.4 Route interactive, ACP, daemon, Web/IPC, and bounded ingress through the owning session supervisor where semantics overlap. Documentation impact: updated private protocol/supervisor/daemon architecture and the public sessions page; no command syntax or canonical snippets changed.
- [x] 1.5 Add a compatibility adapter that submits loop terminal/session intents to the kernel state machine; complete loop reduction in Slice 4. Documentation impact: internal protocol/supervisor ownership only; no public commands, site behavior, or snippets changed.
- [x] 1.6 Add lost/coalesced event, restart, cursor, second-turn, cancellation, and exactly-once terminal regressions. Documentation impact: regression coverage and strict duplicate-event rejection reinforce the existing protocol; no public commands, site behavior, or snippets changed.
- [x] 1.7 Co-deliver session protocol/recovery docs and applicable operator-facing state, resume, cancellation, and client behavior documentation. Documentation impact: corrected private protocol/supervisor/daemon/IPC/Web contracts and public session, migration, and cancellation guidance; no command syntax or canonical snippets changed.

## 2. Composition-authoritative contribution graph
<!-- specs: kernel-composition/documentation, runtime-capabilities/declarations, runtime-contributions/lifecycle -->

- [x] 2.0 Refine Slice-2 ownership, red tests, composition-generation semantics, and the graph-to-legacy dispatch boundary. Documentation impact: internal design/spec clarification only; no public behavior or commands changed.
- [x] 2.1 Add renderer-neutral declaration/generation/diagnostic contracts plus serialization fixtures for dependencies, conflicts, owner tier, trust/confinement, lifecycle, effects, protocol range, timeout, retry, idempotency, transition metadata, and surface support. Documentation impact: updated the owning composition/admission architecture and release notes; this contract-only lane changes no public commands, configuration, site behavior, or canonical snippets.
- [ ] 2.2 Add a pure deterministic candidate-graph builder with all-error diagnostics for duplicate IDs/owners, ambiguous invocations, cycles, missing requirements, conflicts, protocol/platform incompatibility, dangling aliases/groups, and undeclared effects.
- [ ] 2.3 Split static setup into discovery, declaration, validation, activation planning, readiness, and publication phases; adapt existing features before activation and derive legacy EventBus registrations only from the promoted graph.
- [ ] 2.4 Add versioned non-executable dynamic-contribution preflight and separate trusted-code/confinement admission from capability admission before extension, plugin, script, or MCP code can run.
- [ ] 2.5 Add quarantined negotiation, frozen declarations, readiness deadlines, rollback-covered candidate resources, typed health/crash/backoff/degradation/drain/retirement/quarantine/cleanup states, and atomic generation promotion; keep invocation leasing deferred to Slice 3.
- [ ] 2.6 Bind new sessions to a composition generation distinct from process instance identity, retain existing generation values as opaque legacy IDs, and expose effective graph, owner provenance, health, denial reasons, cleanup assurance, and compatibility-dispatch status through one shared semantic diagnostic projection.
- [ ] 2.7 Co-deliver contribution authoring, trust/confinement, lifecycle-state, diagnostics, and public extension/plugin documentation.

## 3. Crash-consistent privileged invocation
<!-- specs: kernel-composition/documentation, runtime-invocation/leases -->

- [ ] 3.1 Move policy/RBAC/approval combination and generation-bound lease issuance into one kernel invocation service.
- [ ] 3.2 Replace tool-name authority with declared effects, principals, timeout, parallelism, retry, and transaction metadata.
- [ ] 3.3 Persist `Prepared` before leasing and `Dispatched` before owner handoff; propagate stable call and deduplication IDs.
- [ ] 3.4 Persist acknowledgement and terminal settlement; recover unsettled dispatched calls as unknown completion.
- [ ] 3.5 Fence further mutation when settlement durability fails and retain emergency recovery evidence.
- [ ] 3.6 Deny retry of mutating unknown-completion calls without owner-enforced idempotency/deduplication.
- [ ] 3.7 Route tools, actions, extensions, host effects, and privileged internal calls through the same lease path while retaining direct pure/read-only service calls.
- [ ] 3.8 Co-deliver invocation/admission developer docs and public permission, retry, unknown-completion, and recovery guidance.

## 4. Provider and loop seams
<!-- specs: kernel-composition/documentation, provider-routing/leases, runtime-session/authority, runtime-invocation/leases -->

- [ ] 4.1 Define provider contributions that bind identity, inventory, auth class, schema dialect, bridge factory, and fallback compatibility.
- [ ] 4.2 Make one provider route service and recorded route lease authoritative across interactive, daemon, child, and bounded execution.
- [ ] 4.3 Reduce the loop to a release-coupled driver over session, route, context, and invocation contracts.
- [ ] 4.4 Remove concrete provider, tool, memory, lifecycle, and frontend names from loop policy.
- [ ] 4.5 Restrict driver replacement to boot or an explicit quiescent session boundary.
- [ ] 4.6 Co-deliver provider contribution/routing docs and public model selection, fallback, authentication, and route-evidence guidance.

## 5. Complete semantic event spine
<!-- specs: kernel-composition/documentation, runtime-session/authority -->

- [ ] 5.1 Extend minimum supervisor facts with model-context provenance, route/schema generations, assistant stream/message, tool calls/results, and step boundaries.
- [ ] 5.2 Extend the Slice-1 compatibility and recovery rules for full-spine context, route, assistant, tool, step, compaction, and projection events without redefining baseline authority semantics.
- [ ] 5.3 Derive provider history, transcripts, frontend snapshots, and compaction checkpoints from semantic events.
- [ ] 5.4 Migrate whole-file session snapshots, metadata checkpoints, narrative journal, and audit consumers without conflating their existing authority.
- [ ] 5.5 Add late, lagged, disconnected, restarted, and corrupted-consumer recovery fixtures.
- [ ] 5.6 Co-deliver event/replay/persistence compatibility docs and public session resume, migration, and recovery documentation.

## 6. Optional domain extraction
<!-- specs: kernel-composition/documentation, runtime-contributions/content-packs, runtime-contributions/lifecycle, runtime-invocation/leases -->

- [ ] 6.1 Convert memory, lifecycle, plans/work, behavior, context/compaction, codescan, and Git integration to declared in-process services.
- [ ] 6.2 Remove concrete feature imports from semantic surfaces and bind TUI, ACP, Web, IPC, CLI, and daemon to shared snapshots/actions.
- [ ] 6.3 Unify native extension, MCP, and manifest discovery under contribution lifecycle while retaining transport-specific adapters.
- [ ] 6.4 Move shipped skills, prompts, personas, tones, workflows, and catalog data into independently versioned content packs.
- [ ] 6.5 Prove each optional domain can be absent or degraded without blocking the maintenance executable or constitutional kernel.
- [ ] 6.6 Co-deliver per-domain architecture, contribution-pack, absence/degradation, and applicable public feature documentation in each extraction lane.

## 7. Release composition, budgets, and deletion
<!-- specs: kernel-composition/documentation, kernel-composition/maintenance, kernel-composition/release-locks, runtime-contributions/lifecycle -->

- [ ] 7.1 Produce signed contribution locks containing identity, artifact digest, protocol range, target support, required/optional status, and fallback behavior.
- [ ] 7.2 Gate maintenance, interactive, headless, daemon, and full composition matrices across supported packaging paths.
- [ ] 7.3 Enforce dependency, binary-size, startup-task, schema-token, resident-capability, and default-callable budgets.
- [ ] 7.4 Remove legacy disabled-name sets, collision-by-order, duplicate supervisor/command authorities, and surface-specific capability allowlists.
- [ ] 7.5 Run broad Rust gates, package/link verification, real maintenance diagnosis, denial/quarantine, stale-record-pruning, audit, and offline-verification exercises, and normal harness exercises.
- [ ] 7.6 Reconcile OpenSpec, design, Workbench, changelog, and release evidence before archive.
- [ ] 7.7 Validate the complete public docs site, canonical snippets, packaging/install guidance, migration notes, and cross-surface terminology against released artifacts.
