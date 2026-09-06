## 1. Budgeted planning
<!-- specs: context-retention -->

- [x] Reproduce oversized recent history with a failing planner test.
- [x] Implement token-aware whole-turn selection, tool exchange boundaries, cancellation, and explicit protected-group exceptions.
- [x] Preserve prior summary input and prove application evicts the planned messages.

## 2. Production integration
<!-- specs: context-retention -->

- [x] Derive the target from effective assembly policy and known system context with summary headroom; test small windows and saturation.
- [x] Wire loop, feature-requested, overflow, and manual paths to the budget and selected application.
- [x] Align current authoritative sources before compaction, preserve post-request results, and validate retained source replay.
- [x] Run focused regressions, crate landing tests, and Clippy; record evidence and limits.
- [x] Update Unreleased and parity comparison, commit the bounded change, validate and close OpenSpec.
