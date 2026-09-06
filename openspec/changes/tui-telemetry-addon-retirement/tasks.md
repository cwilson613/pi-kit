# Implementation tasks

This retirement is planned; no deletion or addon implementation is claimed.

## 1. Retire the core surface
<!-- specs: core-tui-telemetry -->

- [ ] Inventory instrument, glyph and animation consumers; separate visualization-only state from operational telemetry and record deletion owners.
- [ ] Add failing geometry/default tests for both entries and both detail levels, plus legacy instrument-toggle behavior.
- [ ] Remove inference/tools panel allocation and decorative engine symbol chains; provide compact actionable text where required.
- [ ] Delete exclusive renderer/update/animation state, timers, unused dependencies and superseded tests. Retain shared facts with remaining consumers.
- [ ] Verify zero decorative frame demand at idle and retain decision/cancellation/completion/draft regressions.

## 2. Migration, optional-addon boundary and evidence
<!-- specs: core-tui-telemetry -->

- [ ] Update UI menus/help, profile compatibility, snapshots and current public TUI docs; legacy instrument requests report retirement explicitly.
- [ ] Confirm future addon capability requirements against existing extension lifecycle without adding dormant core widgets or a speculative SDK.
- [ ] Build and capture both entries before/after through the headless PTY runner; inspect reclaimed rows and actionable controls. Reserve native GUI comparison for a dedicated compatibility session.
- [ ] Run the omegon crate, Clippy and applicable script/schema gates; update Unreleased and verify every scenario before lifecycle completion.
