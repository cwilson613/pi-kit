## 1. Positive kernel boundary
<!-- specs: kernel-composition/artifact-profiles -->

- [ ] Add a failing policy test that inventories every dependency and resident capability in `kernel-only`.
- [ ] Define the first positive kernel allowlist or owner-budget policy.
- [ ] Build and start the kernel-only artifact with an isolated state root.
- [ ] Execute one useful core operation and verify every optional-domain absence contract.

## 2. Kernel plus codescan
<!-- specs: kernel-composition/artifact-profiles -->

- [ ] Add a failing row that expects codescan restoration from the separately built sidecar.
- [ ] Compose the admitted codescan extension without changing the host binary graph.
- [ ] Reuse the host conformance driver to index and search a fixture workspace.
- [ ] Verify inventory and callable-surface deltas contain only declared codescan additions.

## 3. Accumulated product matrix
<!-- specs: kernel-composition/artifact-profiles -->

- [ ] Add failing matrix tests that reject runtime labels over an unchanged artifact as distinct rows.
- [ ] Define source, linked, and release execution for kernel-only, additive, and full-product rows.
- [ ] Require each future extracted domain to add absence, restoration, and accumulated-product assertions.
- [ ] Run the full matrix with deterministic isolated state and bounded process cleanup.

## 4. Aggregate budgets
<!-- specs: kernel-composition/artifact-profiles -->

- [ ] Add failing budget tests for an oversized host, sidecar, or aggregate installation.
- [ ] Measure dependency count, binary size, installed size, startup tasks, processes, schema tokens, and capabilities by owner.
- [ ] Record target-specific baselines and bounded deltas for each composition row.
- [ ] Add budget enforcement to pull-request or nightly CI according to runtime cost.

## 5. Documentation and verification
<!-- specs: kernel-composition/artifact-profiles -->

- [ ] Document how an extracted domain adds a row without weakening prior rows.
- [ ] Run kernel, additive, full-product, dependency, budget, formatting, and lint gates.
