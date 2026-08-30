## 1. Strict component policy resolution
<!-- specs: settings/component-policy -->

- [x] Add failing settings tests for composition defaults, explicit profile enable/disable, `core:*`, deny-wins precedence, and child deny propagation.
- [x] Add failing validation tests for unknown fields, malformed selectors, unknown exact IDs, invalid values, and non-disableable targets.
- [x] Implement typed profile and user-local component policy parsing with one canonical resolver and source provenance.
- [x] Reconcile the runtime validator and `pkl/Profile.pkl` or its replacement schema so editor and runtime acceptance match.
- [x] Add failing migration tests and migrate legacy `extensions.disabled = ["omegon-codescan"]` without altering unrelated SDK-extension policy.

## 2. Pre-spawn lifecycle enforcement
<!-- specs: runtime-contributions/lifecycle -->

- [x] Add a failing release-layout test proving denied codescan performs no spawn, handshake, readiness probe, index, database, or worker mutation.
- [x] Apply effective component policy before candidate probing and graph publication, preserving unrelated component admission.
- [x] Add failing dependency-graph tests for deterministic optional-dependent omission and rejection of a disabled required owner.
- [x] Preserve active-session generation immutability and report restart-required behavior after profile mutation.

## 3. Disabled service behavior
<!-- specs: runtime-contributions/lifecycle -->

- [x] Add failing tests that remove disabled component tools from the model-callable set while retaining stable direct invocation contracts.
- [x] Return typed `service:disabled` with component and policy provenance across tool, CLI, and ACP invocation paths.
- [x] Add an enable-after-restart test proving the same packaged codescan component becomes healthy and searchable without reinstalling it.

## 4. Commands and observability
<!-- specs: settings/component-policy, runtime-contributions/lifecycle -->

- [x] Add failing command-registry tests for component view/enable/disable parity across TUI, CLI remote execution, and ACP.
- [x] Fix source-aware profile mutation so named active profiles are updated instead of shadowed by a lower-precedence legacy file.
- [x] Add effective-policy projections that distinguish packaged, disabled-by-profile, absent, incompatible, failed, quarantined, and healthy states.
- [x] Verify diagnostics identify the exact profile, user-local, or propagated source that determined the outcome.

## 5. Packaging and compliance acceptance
<!-- specs: kernel-composition/artifact-profiles -->

- [x] Add a failing installed full-product test proving profile disablement leaves archive inventory, digests, locks, and atomic rollback validity unchanged.
- [x] Add a failing distribution-parity scenario using isolated default settings for positive activation and explicit deny settings for negative activation.
- [x] Document component policy, restart semantics, non-disableable kernel capabilities, legacy migration, and the distinction from SDK-extension state.
- [x] Run settings, kernel composition, native host, codescan acceptance, distribution policy, formatting, lint, and multi-crate landing gates.
