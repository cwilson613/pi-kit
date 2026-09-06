# Implementation tasks

All tasks are planned. Validation of this corpus does not demonstrate runtime behavior.

## 1. Quiet startup and failure guidance
<!-- specs: provider-connections -->

- [ ] Add failing projection tests for zero/one/many providers, expired selected credentials, unrelated missing credentials, and selected/serving fallback state across both layouts and detail levels.
- [ ] Introduce compact interactive startup policy in bootstrap_projection/setup; consolidate duplicate startup credential warnings while preserving explicit control/ACP diagnostics.
- [ ] Add a failing NullBridge response test; replace suggested-provider enumeration with concise /connect guidance.
- [ ] Verify output remains bounded as provider inventory grows and update only affected snapshots.

## 2. Shared connection menu and command migration
<!-- specs: provider-connections -->

- [ ] Add failing tests for Connections/Add provider grouping, expired and external credentials, empty state, filtering, and route badges using existing status projections.
- [ ] Adapt auth_menu_projection and App menu entry to the two views; reuse shared menu interaction and inline terminal ownership.
- [ ] Add failing /connect dispatch and registry/control authorization tests, including unknown providers and unsupported secure remote interaction.
- [ ] Register /connect and route direct setup through existing auth handlers; preserve /login and /auth login compatibility and internal protocol identifiers.
- [ ] Add secret-input and cancellation regressions; separate explicit API-key console opening from setup and verify browse/search cannot open a browser.
- [ ] Update auth/route/main/footer remediation, command help, and relevant public documentation to prefer /connect; retain /model and /logout semantics.

## 3. Captured acceptance and handoff
<!-- specs: provider-connections -->

- [ ] Extend isolated headless PTY acceptance for quiet startup, connection discovery, search, cancel/draft restoration, and one stubbed credential flow in both layouts and detail levels; record build identity and inspect captures.
- [ ] Run just test-crate omegon, just clippy-changed, and any affected script tests; record actual results and limitations.
- [ ] Update Unreleased for implemented behavior, verify every scenario, and reconcile OpenSpec tasks with evidence. Keep the future /login renewal proposal separate.
