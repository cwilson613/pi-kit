## 1. Highest ROI — Complete project instruction loading
<!-- specs: harness-parity/opencode2 -->

- [ ] Add failing prompt.rs fixtures for root/intermediate/cwd composition and a root file longer than 4000 bytes, including multibyte UTF-8.
- [ ] Verify find_repo_root behavior in a linked worktree; add boundary, canonical duplicate, absent-file, unreadable-file, and non-Git cwd fixtures.
- [ ] Replace first-file selection with root-to-cwd discovery and source-labelled complete content; preserve existing global guidance ownership.
- [ ] Remove silent truncation and propagate actionable read/preparation failures through existing prompt callers.
- [ ] Verify complete guidance fits the existing model request budget or fails before dispatch; add a no-network-dispatch assertion for oversized required guidance.
- [ ] Run focused prompt/preparation tests, the applicable crate landing gate, and Clippy; exercise nested-worktree prompt construction through the current harness.
- [ ] Document discovery order and construction-time refresh scope; update Unreleased and commit this slice independently.

## 2. Next ROI — Separate MCP phase deadlines
<!-- specs: harness-parity/opencode2 -->

- [ ] Add failing configuration fixtures for phase overrides, legacy fallback, absent settings, invalid explicit values, and duration overflow; record existing legacy-zero behavior.
- [ ] Add optional startup_timeout_secs, catalog_timeout_secs, and execution_timeout_secs at McpServerConfig and applicable Pkl schemas.
- [ ] Apply startup and catalog deadlines to their complete phases, including catalog pagination, while preserving managed outer lifecycle bounds.
- [ ] Apply execution deadlines to tools, resource reads, and prompt retrieval without extending them on progress.
- [ ] Use fake MCP fixtures to verify slow execution after fast discovery, stalled startup/catalog, pagination exhaustion, and timeout phase diagnostics.
- [ ] Verify cancellation settles promptly, cleanup remains process-tree scoped, remote uncertainty is explicit, and unrelated calls are not killed by a single-call timeout.
- [ ] Run focused MCP/configuration tests, schema checks where applicable, the landing gate, and Clippy; exercise the current harness with the fake server.
- [ ] Document effective budget precedence and diagnostics; update Unreleased and commit this slice independently.

## 3. Close the bounded pass
<!-- specs: harness-parity/opencode2 -->

- [ ] Reconcile each scenario with local test and runtime evidence, including build identity and remaining limitations.
- [ ] Update the comparison to distinguish implemented local fixes from unverified beta executable parity; do not advance a full upstream review marker.
- [ ] Validate OpenSpec, reconcile any active Workbench state, and archive only after both implementation slices are complete.
