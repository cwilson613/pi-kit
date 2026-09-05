# First delivery verification

The reconstruction is implementing. Project navigation and unified interaction ownership remain pending.

## Interactive evidence

On 2026-09-05, the agent launched and drove the real debug binary in a private tmux server on macOS. Source base was `70066812e9e6e62b1b3b50eab47dd10c3426b9ed` with the working changes identified in each manifest.

- `/tmp/omegon-tui-acceptance-01`: first reply appeared; second prompt timed out. Launch inherited the repository cwd.
- `/tmp/omegon-tui-acceptance-02`: corrected terminal cwd to the isolated workspace; same stall. Process sampling identified `acknowledge_stream_presentation_draw` repeatedly reclassifying deferred events.
- Regression test `drawn_replay_makes_progress_with_multiple_deferred_events` failed before the fix with “released completion must not requeue behind its successor.”
- `/tmp/omegon-tui-acceptance-03`: after repair, two replies and resize passed. Inspected the rendered cells; both replies and `ready · idle` were visible.
- `/tmp/omegon-tui-acceptance-04`: strengthened process/action provenance and explicit idle assertion. Passed in 13.89 seconds with two local provider requests, no paid inference, and no forced cleanup. Inspected final 90×30 capture.

Executable: `target/debug/omegon`, SHA-256 `6072b663adcd3c841d1947ff6668e090222894777779d6dceef1985a80f80467`. Build: `just test-tui-captured /tmp/omegon-tui-acceptance-03`. Final runner invocation used that same artifact. Raw evidence is machine-local and intentionally excluded from Git.

In the first delivery, the fresh-session screen reported an unavailable semantic projection (missing cursor or empty authority stream). The run establishes live turn completion and input recovery, not correct project/session projection initialization. The second delivery below resolves startup ordering and adds explicit first-turn readiness.

## Checks

- Python fixture contract tests passed via `python3 scripts/tests/test_tui_acceptance.py`. The local interpreter lacks pytest; the file also supports direct execution.
- Focused TUI suite: 1,239 passed, zero failed, one ignored with canonical glyph environment. Controller and App replay regressions passed, alongside authoritative completion/idle queue and second-submission tests.
- `just clippy-changed`: passed for Omegon, all targets.
- Initial crate gate: 5,080 passed, one recovery campaign failed its 15-second wall-clock assertion under concurrent build load, ten ignored. No timing assertion changed. The complete `env -u NO_COLOR -u OMEGON_ASCII_GLYPHS RUST_TEST_THREADS=4 just test-crate omegon` rerun passed: 5,106 passed, zero failed, 11 ignored across nine suites, including the recovery campaign and all integration targets.
- OpenSpec structural validation passed; it does not imply completion of pending shell tasks.

Implementation commit: `b8ca3287` (replay repair, regression tests, terminal runner, and recipe). No installed launcher was changed.


# Second delivery: session readiness and responder ownership

Session-backed startup now opens authority and prepares validated projections before TUI and IPC launch. Empty new sessions render “Ready for first turn” without fabricating semantic history. A responder owner serializes permission/manual-action arrivals, retains passive surfaces and their state, and paints the active prompt above other overlays. Profile application now copies permission policy into runtime Settings.

## Test-first evidence

- `initial_view_is_available_before_background_projection_starts` first failed on a missing projection cursor. The final test validates the initial authority-derived view and preserves truthful pre-spine lineage.
- `blocking_decisions_preserve_copy_surface_and_resolve_in_arrival_order` first failed because a later manual-action prompt displaced permission. The fixed App test verifies permission then wait responses and preserved Settings selection/copy state.
- `decision_queue_is_bounded_and_overflow_resolves_negatively` verifies 64 queued decisions plus one active owner; overflow resolves as deny.
- `profile_application_preserves_runtime_tool_permission_policy` failed before profile application copied permission policy; it also verifies switching to an empty policy clears the prior rules.
- Python provider tests cover ordinary streams, unknown routes, and a gated write-tool request.

## Captured terminal evidence

The agent inspected the final startup, permission-over-Settings, restored Settings, and completed-turn captures in `/tmp/omegon-tui-interaction-05/`. The run passed in 14.87 seconds with four loopback provider requests, no paid inference, no denied file created, and no forced cleanup. The owned process group was independently confirmed absent.

Build: `just test-tui-captured /tmp/omegon-tui-interaction-05`. Binary SHA-256: `ae2e55f03e4b1ca8d7e9b41f8758914afffe27dcd14f5b5107bbc196c25909cb`. The manifest records base revision `7f3b5aa9` plus dirty source state, absolute executable/process identity, input actions, timestamps, dimensions, and eight capture hashes.

Earlier extended runs informed the fixture:

- `interaction-01`: temporary paths are allowed by default, so no permission was requested.
- `interaction-02`: explicit write/prompt policy exposed missing profile-to-runtime permission propagation.
- `interaction-03`: the repaired policy produced a visible denied write, but a tiny fixture final answer triggered no-progress continuations and exceeded the four-request assertion.
- `interaction-04`: a substantive denial summary completed the scenario; a process-existence signal probe raised EPERM during cleanup and prevented manifest writing. The runner now reads process-group existence from the process table and writes the manifest even if cleanup fails.
- `interaction-05`: explicit ready-state startup, denied write and Settings round trip, four requests, and cleanup all passed.

## Validation and remaining scope

The serialized Omegon crate gate passed: 5,110 tests, zero failures, 11 ignored across nine suites. It includes the initial-view, profile-policy, responder-ordering, queue-bound, and integration regressions. The final empty-session display wording was subsequently exercised in the real captured run; the final serialized TUI gate passed 1,241 tests with zero failures and one ignored. `just clippy-changed` passed for Omegon/all targets. Python fixture tests and OpenSpec validation passed.

Concurrent test runs reported an extension handshake failure and a skills-menu inventory mismatch. The serialized crate gate passed those tests; no unrelated test assertions were changed to accommodate them.

This delivery does not complete generic navigation ownership, extension interaction routing, stable selection across domain refresh, the inline/fullscreen terminal coordinator, or project/session/work navigation. Those tasks remain unchecked. The captured prompt still uses the existing compact layout and labels; broader layout and affordance review remains part of shell reconstruction.

Implementation commits: `5d96f966` (startup view), `59c30d17` (profile policy), and `23640c20` (decision ownership and captured permission round trip). No installed launcher was changed.
