## 1. Protocol conformance fixture
<!-- specs: runtime-contributions/lifecycle -->

- [ ] Add failing tests for missing or unsupported SDK identity, malformed tools, failed bootstrap, and failed readiness.
- [ ] Implement a deterministic native fixture with delay, crash, child-process, and tool-shape controls.
- [ ] Extract a reusable conformance driver from existing host extension tests without changing production policy.
- [ ] Run the generic handshake campaign against the fixture and every first-party native extension.

## 2. Real host-to-codescan acceptance
<!-- specs: runtime-contributions/lifecycle -->

- [ ] Add a failing test that installs the real codescan extension in a production-like discovery root.
- [ ] Make codescan advertise the generic SDK contract and pass host readiness validation.
- [ ] Invoke index and search through the host binding and assert the expected source hit and process provenance.
- [ ] Remove the sidecar and verify typed unavailability while unrelated host work remains available.

## 3. Cancellation and replacement
<!-- specs: runtime-contributions/lifecycle -->

- [ ] Add a failing test that cancels a codescan request after work has started.
- [ ] Propagate cancellation through the host handle, wire notification, worker, and typed outcome.
- [ ] Add failing tests for stable-shape replacement, changed-shape refusal, and stale-generation invocation.
- [ ] Verify a replacement leaves one admitted process and preserves subsequent invocation.

## 4. Crash isolation and cleanup
<!-- specs: runtime-contributions/lifecycle -->

- [ ] Add failing tests for one extension crash while another extension and the host remain usable.
- [ ] Add failing tests for restart-budget exhaustion, quarantine, and runtime-doctor evidence.
- [ ] Add a descendant process fixture and first prove the descendant survives the current direct-child assertion.
- [ ] Enforce and verify bounded process-tree settlement on refusal, replacement, cancellation, and shutdown.

## 5. Conformance gate
<!-- specs: runtime-contributions/lifecycle -->

- [ ] Add the reusable campaign to first-party extension CI.
- [ ] Keep focused fixture tests separate from the slower real-process acceptance row.
- [ ] Run extension, host adapter, platform, formatting, and changed-code lint gates.
