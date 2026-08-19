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

## 1. Minimum durable session authority
<!-- specs: kernel-composition/documentation, runtime-session/authority -->

- [x] 1.1 Approve the minimum semantic event vocabulary, sequence/version rules, and compatibility policy.
- [x] 1.2 Add the approved adjacent authority stream, strict reducer/cache, and durable prompt, queue, turn, interruption, minimum invocation, recovery, and terminal facts.
- [x] 1.3 Refactor the existing supervisor scaffold into one frontend-neutral compiled implementation instantiated once per session. Documentation impact: internal architecture only; no public commands, site pages, or snippets changed.
- [x] 1.4 Route interactive, ACP, daemon, Web/IPC, and bounded ingress through the owning session supervisor where semantics overlap. Documentation impact: updated private protocol/supervisor/daemon architecture and the public sessions page; no command syntax or canonical snippets changed.
- [x] 1.5 Add a compatibility adapter that submits loop terminal/session intents to the kernel state machine; complete loop reduction in Slice 4. Documentation impact: internal protocol/supervisor ownership only; no public commands, site behavior, or snippets changed.
- [ ] 1.6 Add lost/coalesced event, restart, cursor, second-turn, cancellation, and exactly-once terminal regressions.
- [ ] 1.7 Co-deliver session protocol/recovery docs and applicable operator-facing state, resume, cancellation, and client behavior documentation.

## 2. Composition-authoritative contribution graph
<!-- specs: kernel-composition/documentation, runtime-capabilities/declarations, runtime-contributions/lifecycle -->

- [ ] 2.1 Extend declarations with dependencies, conflicts, owner tier, lifecycle, effects, protocol range, timeout, retry, idempotency, and transition metadata.
- [ ] 2.2 Collect declarations before ordinary activation and reject duplicate IDs, ambiguous invocations, cycles, missing owners, and unsupported requirements.
- [ ] 2.3 Add static dynamic-contribution preflight and separate trusted-code/confinement admission from capability admission.
- [ ] 2.4 Add quarantined protocol negotiation, frozen declaration sets, readiness deadlines, and atomic generation promotion.
- [ ] 2.5 Add typed heartbeat loss, dependency degradation, crash/backoff, drain, retirement, quarantine, and forced cleanup.
- [ ] 2.6 Expose the effective graph, owner provenance, health, denial reasons, and generation through shared diagnostics.
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
