## 1. Positive kernel boundary
<!-- specs: kernel-composition/artifact-profiles -->

- [x] Add a failing policy test that inventories every dependency and resident capability in `kernel-only`.
- [x] Define the first positive kernel allowlist or owner-budget policy.
- [x] Build and start the kernel-only artifact with an isolated state root.
- [x] Execute one useful core operation and verify every optional-domain absence contract.

## 2. Kernel plus codescan
<!-- specs: kernel-composition/artifact-profiles -->

- [x] Add a failing row that expects codescan restoration from the separately built sidecar.
- [x] Compose the admitted codescan extension without changing the host binary graph.
- [x] Reuse the host conformance driver to index and search a fixture workspace.
- [x] Verify inventory and callable-surface deltas contain only declared codescan additions.

## 3. Accumulated product matrix
<!-- specs: kernel-composition/artifact-profiles -->

- [x] Add failing matrix tests that reject runtime labels over an unchanged artifact as distinct rows.
- [x] Extract native-extension manifest, protocol transport, and canonical process supervision into a dependency-clean workspace crate used by the full product.
- [x] Define source execution for kernel-only, additive, and full-product rows.
- [x] Require each future extracted domain to add absence, restoration, and accumulated-product assertions.
- [x] Run the full matrix with deterministic isolated state and bounded process cleanup.

## 4. Aggregate budgets
<!-- specs: kernel-composition/artifact-profiles -->

- [x] Add failing budget tests for an oversized host, sidecar, or aggregate installation.
- [x] Measure dependency count, binary size, installed size, startup tasks, processes, schema tokens, and capabilities by owner.
- [x] Record target-specific baselines and bounded deltas for each composition row.
- [x] Add budget enforcement to pull-request or nightly CI according to runtime cost.

## 5. Documentation and verification
<!-- specs: kernel-composition/artifact-profiles -->

- [x] Document how an extracted domain adds a row without weakening prior rows.
- [x] Run kernel, additive, full-product, dependency, budget, formatting, and lint gates.
