## 1. Operational reduced kernel
<!-- specs: kernel-composition/artifact-profiles -->

- [x] Add failing unit and installed-artifact tests for a deterministic bounded conformance turn, missing terminal completion, and post-terminal events.
- [x] Implement the no-network scripted turn with explicit request, event, deadline, and terminal bounds without growing the kernel dependency roots.
- [x] Extend the additive artifact ladder to execute the same kernel turn before and after codescan composition and prove unrelated behavior is unchanged.
- [x] Add a failing acceptance test for a provider-backed reduced-kernel bounded turn using the shared route and terminal contracts.
- [x] Extract and reuse the minimum provider-neutral runtime contracts. Make the reduced kernel complete the admitted bounded turn without importing product domains.
- [x] Verify typed route evidence, structured completion, cancellation, budget exhaustion, process cleanup, and the kernel dependency/budget ratchets.

## 2. First-party core classification
<!-- specs: kernel-composition/artifact-profiles -->

- [x] Add a failing policy test requiring every first-party domain to appear exactly once in a machine-readable packaging-class inventory.
- [x] Classify constitutional residents, host services, signed `core:*` components, shipped content, and SDK extensions with owner and extraction disposition.
- [x] Make composition and documentation checks reject unknown domains, duplicate ownership, or a `core:*` promotion without additive-ladder evidence.
- [x] Record follow-up extraction candidates without extracting domains whose authority, cost, or failure boundary does not justify a separate component.

## 3. Distribution trust parity
<!-- specs: kernel-composition/distribution-parity -->

- [x] Add direct-installer red tests for valid-checksum candidates with absent, invalid, wrong-target, wrong-version, or composition-mismatched signed evidence. Assert unchanged activation and no retained staging.
- [x] Define one bootstrap-verifier adapter contract and deterministic fixtures for external first-install verification and active-generation maintenance verification.
- [x] Make the direct installer fetch and externally authenticate the archive, package manifest, and signature bundle before extraction. Fail closed when tooling or evidence is unavailable.
- [x] Revalidate the extracted direct-install composition before activation and clean download plus generation staging on every refusal path.
- [x] Add switch red tests proving candidate maintenance code cannot verify itself and mixed host, component, content, receipt, or lock generations cannot publish.
- [x] Route switch verification through the active generation's maintenance authority, then preserve the single atomic generation selector for publication and recovery.
- [x] Add scope guards and interruption tests that remove failed direct-install and switch staging without changing the active generation.
- [x] Add failing Nix policy tests for unpinned or falsely full-product composition and enforce its authenticated host-only boundary.
- [x] Add OCI policy fixtures for digest mismatch and missing signature, SBOM, provenance, or composition identity. Require one explicit host-only or full-product class.
- [x] Wire the OCI policy verifier into CI readiness without claiming production attestation publication.
- [x] Run archive, installer, switch, rollback, and deterministic distribution-policy acceptance with isolated state and cleanup. Defer live Homebrew, Nix, and OCI release-lane acceptance to stable-release work.
- [x] Disable automatic Homebrew and OCI publication from stable tag pushes; retain an explicit manual opt-in for future channel bring-up.

## 4. Runtime parity and bounded authority
<!-- specs: runtime-session/authority, runtime-contributions/lifecycle -->

- [x] Add one full-lineage semantic fixture with session and composition identity, activity revision, queue, active and terminal turns, lifecycle health, canonical actions, owner, and denial reason.
- [x] Add a failing matrix that projects the same fixture through TUI, ACP, Web, IPC, CLI, and daemon adapters and requires explicit declarations for unsupported fields.
- [x] Introduce a renderer-neutral versioned activity projection and revision comparison that prevents stale or unversioned queue state from overriding newer durable closure.
- [x] Migrate TUI, ACP, Web, IPC, and daemon activity plus action projection to the shared DTO. Make one-shot CLI declare persistent reconciliation unsupported while preserving representable parity.
- [x] Add per-edge missed-advice tests that reconcile to idle, admit a second prompt, and ignore delayed terminal advice for the prior turn.
- [x] Add bounded-run red tests for manifest-admitted time, turn, token, and tool limits at below, exact, and one-above boundaries.
- [x] Provide prospective tool-budget authority in the reduced kernel, admit `tool_budget` through task-capsule manifests, and reserve the shared budget before invocation preparation, lease creation, or owner dispatch.
- [x] Return typed structured exhaustion with admitted and observed limits after route, invocation, provider, process, and terminal authority settle.
- [x] Derive native extension contribution-generation identity from the admitted source digest and represent one active plus one pending generation per contribution.
- [x] Add lifecycle state-machine tests proving candidate C settles and replaces pending B without changing active A or exposing either candidate.
- [x] Add a supervisor-owned publication coordinator that explicitly commits a fully staged candidate only after idle, active-call, queue, and unknown-invocation guards pass.
- [x] Prove turn closure and the next turn start do not implicitly publish a pending contribution generation.
- [x] Add a real-process A-to-B replacement test. Keep B hidden while active, publish B at explicit quiescence, deny stale A authority before RPC, and route fresh work only to B.
- [x] Fence extension polling handles and aliases through the shared generation admission table before native RPC owner entry.
- [x] Add remote cleanup projections and tests that settle host resources but report remote state as best-effort or unverified without a new acknowledgement protocol.

## 5. Documentation reconciliation
<!-- specs: kernel-composition/documentation -->

- [x] Audit architecture, command, install, security, extension, session, and packaging claims against named executors from lanes 1-4.
- [x] Document the required external bootstrap verifier, active-generation switch verifier, checksum limitation, and fail-closed staging cleanup.
- [x] Document supervisor-coordinated quiescent publication, single pending-candidate replacement, generation-fenced handles, and best-effort remote cleanup.
- [x] Document the shared activity revision contract and the one-shot CLI persistent-reconciliation limitation.
- [x] Label OCI policy and CI verification separately from production attestation publication.
- [x] Update public pages, canonical snippets, CLI help, and examples only after their production behavior and executor land.
- [x] Run durable-doc checks, policy-to-document inventory checks, site tests, snippet checks, link checks, and the production site build.

## 6. Scenario-indexed acceptance corpus
<!-- specs: milestone/readiness-gate -->

- [x] Define stable scenario families, orthogonal dimensions, authority invariants, fault phases, cleanup oracles, and a constrained sentinel matrix for kernel, core-component, and SDK-addon acceptance.
- [x] Add a machine-readable corpus and structural validator that distinguish implemented executors from planned evidence.
- [x] Bind `BND-004` to a manifest-driven bounded native-RPC executor with exact-boundary and one-above owner-entry evidence.
- [x] Bind `CMP-003` to a generic core-qualification policy executor that rejects missing, aliased, or cross-component evidence.
- [x] Bind `LIF-001` and `LIF-003` to state-machine plus real-process changed-generation executors with owner-entry markers.
- [x] Bind `SUR-001` and `SUR-002` to the shared projection matrix and missed-advice second-prompt campaign.
- [x] Bind `DST-001`, `DST-002`, and `DST-004` to installer, switch, and OCI policy executors that prove activation preservation and cleanup.
- [x] Bind `CLN-002` to typed remote-cleanup projection evidence without claiming remote settlement.
- [x] Require the provider-kernel, signed-core, SDK-addon, and milestone promotion profiles in ordinary CI without activating deferred publication lanes.

## 7. Milestone PR-readiness gate
<!-- specs: milestone/readiness-gate -->

- [x] Verify every delta scenario against named automated or bounded runtime evidence and check every implementation task.
- [x] Run focused crate and contract tests after each production slice. Run composition, distribution, semantic-parity, extension-lifecycle, formatting, and documentation gates before closeout.
- [x] Run `just test-commit` and `just clippy-changed`, recording any unavailable cross-platform evidence without claiming it passed.
  - Local evidence: `RUST_TEST_THREADS=4 just test-commit` and `just clippy-changed` pass on macOS. Live Homebrew, Nix, and OCI release lanes are deferred and are not recorded as passed.
- [x] Update `[Unreleased]`, validate `operational-kernel-core-readiness`, and pass `archive --check` before declaring the branch PR-ready.
