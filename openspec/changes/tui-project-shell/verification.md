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

# Third delivery: navigation and terminal ownership

Passive overlays now derive rendering and keyboard precedence from one navigation owner. Escape dismisses the visible extension overlay while preserving covered copy/Settings state; menu paging cannot scroll the background conversation. Extension action keys and paste cannot modify the composer. Unwired action responses report their limitation and retain the prompt instead of claiming success.

Current fullscreen/native-export terminal mode changes now use one shared handle. It records successful mode operations, restores exact preferences around primary-screen operations, and forces a complete redraw after returning. Startup, mouse preferences, shell suspension, tutorial handoff, and shutdown use the same owner, with the existing nonblocking panic fallback retained. Mouse-mode UI state advances only after the mode operation succeeds. Ordinary shutdown attempts every release and retains failed modes for later cleanup instead of discarding ownership in advance.

## Test-first evidence

- The extension-over-copy Escape and Settings PageDown regressions failed before navigation ownership was shared (`/tmp/omegon-nav-red.log`).
- Injected transition failures first failed against the placeholder terminal state machine (`/tmp/omegon-terminal-red.log`). Final cases cover partial entry, partial leave and retry, failed primary writes with mouse disabled, and failed suspension preventing the primary operation.
- `shutdown_releases_other_modes_and_retains_only_failed_modes_for_retry` failed against the previous consume-before-release cleanup behavior (`/tmp/omegon-terminal-shutdown-red.log`), then drove success-ordered best-effort cleanup.
- The TUI suite passed 1,248 tests, zero failures, one ignored before the final mouse-error reporting, registered-command wording, and shutdown retry adjustments. The final crate gate below covers those adjustments too.

## Captured terminal evidence

The agent inspected `/tmp/omegon-tui-navigation-terminal-03/`: native primary-screen transcript, restored fullscreen replies, permission ownership, restored Settings, and denied tool completion. Ten captures passed in 15.06 seconds with four local provider requests, no paid inference, no denied file created, and no forced cleanup. The owned process group was independently confirmed absent.

Build: `just test-tui-captured /tmp/omegon-tui-navigation-terminal-03`. Binary SHA-256: `fa43d15cdea65e150c351fc123a2b9e3be21707a3f02e5c78c9ca30f7fca1db5`. The manifest records base revision `2d3a859a` plus dirty source state, process identity, actions, capture hashes and geometry. Before and after native export, tmux reported alternate screen enabled and mouse capture disabled (`1:0`). The restored framebuffer visibly contains both replies.

The first run timed out because the runner used the stale `/print` name still present in an existing message. Inspection identified the registered `/session-export scrollback` command; the runner and message were corrected before the successful run. Its failed evidence remains in `/tmp/omegon-tui-navigation-terminal-01/`. Run `-02` then passed the registered-command scenario; final run `-03` repeated it against the completed shutdown implementation.

## Remaining scope

The current client still uses a fullscreen viewport. Persistent inline layout, automatic bounded transcript publication, extension action response transport, and project/session/work navigation remain unchecked in the task plan. Real OS job-control suspension, tutorial process replacement, and injected physical terminal I/O failures are not covered by the captured fixture. Narrow-layout clipping and transient completion footer wording remain existing presentation limitations visible in the captures.

## Final validation

- Final serialized `just test-crate omegon`: 5,118 passed, zero failed, 11 ignored across nine suites (`/tmp/omegon-navigation-terminal-crate-final.log`). Canonical glyph environment: `env -u NO_COLOR -u OMEGON_ASCII_GLYPHS OMEGON_NERD_FONT=1 RUST_TEST_THREADS=1`.
- `just clippy-changed`: passed for Omegon/all targets (`/tmp/omegon-navigation-terminal-clippy-final.log`).
- Terminal owner/input tests: 12 passed, zero failed (`/tmp/omegon-terminal-shutdown-green.log`).
- Python fixture contracts and OpenSpec structural validation passed. The change remains implementing because the task plan retains the next reconstruction work.

Implementation commit: `1718e546`. The final captured binary was built from these code changes before commit; its manifest retains the prior base revision and dirty-source inventory. No installed launcher was changed.

# Fourth delivery: F2 Project browser

F2 now opens Sessions and Work tabs for the current workspace. Enter inspects the current session, saved-session metadata, the active plan's tasks, or a published workstream summary. Saved-session resume requires R from its details and is refused while a turn is active; the request uses the existing session-control route. A successful session view replacement closes the old browser. Escape backs out of details and then returns to the preserved composer. F5 refreshes the snapshot while retaining a surviving row by stable ID. A missing row closes stale detail. Permission decisions leave the browser mounted, and Ctrl+C from the browser cancels without erasing the next draft.

## Verification completed

- The initial `project_browser_inspection_preserves_composer_draft` regression failed before implementation (`/tmp/omegon-project-red.log`).
- Four focused browser tests passed for draft/inspection return, permission return, stable refresh after insertion, and removal of an inspected row (`/tmp/omegon-project-green.log`).
- The subsequent serialized TUI suite passed 1,255 tests, zero failed, one ignored (`/tmp/omegon-project-tui-final.log`). It also includes explicit idle-only saved-session resume and cancellation preserving the browser/draft. Two earlier assertions for the old `/ui surfaces` footer hint were updated to require the new F2 project hint; the detail hotkey remains required.
- A final test for populated plan task details was added and compiled after that TUI run; it has not executed because of the host launch problem below.
- Python fixture contracts and OpenSpec structural validation passed. The extended runner now navigates Sessions/detail/Work with an unsent draft, then denies a write over the Work tab and checks return to that tab.

## Interactive capture and final gate blocked at executable startup

The real terminal attempt `/tmp/omegon-tui-project-browser-01/` timed out before application startup. It produced no application log and made zero provider requests. Its only terminal capture is blank. The manifest records the executable/source identity and failed run; this is not successful browser acceptance.

A macOS process sample (`/tmp/omegon-project-startup-sample.txt`) placed the executable at `_dyld_start + 0`, with a 128 KiB footprint and no application frames. `codesign --verify --verbose=2 target/debug/omegon` succeeded. Standalone `--version` launches of both the original artifact and an identical copy outside the checkout also stalled before entry. Those owned probes were terminated; no host security service or policy was changed.

The final `just test-crate omegon` compiled successfully but its test executable also stopped at `_dyld_start + 0` before announcing or running tests (`/tmp/omegon-project-test-loader-sample.txt`, `/tmp/omegon-project-crate.log`). After sampling the same pre-entry state, the owned test process was terminated. This gate is indeterminate, not a passed gate or a test assertion failure. Do not archive this change or mark captured project acceptance complete until the real run and final gate execute on a functioning host.

Work remains a snapshot of current Workbench plan/workstream summaries. Full work-source inventory, execution/evidence navigation, populated-work runtime capture, persistent inline layout, and the other unchecked reconstruction tasks remain pending.

The `just clippy-changed` generated script launcher also stalled before execution. Its checks were then run directly: `python3 scripts/affected_crates.py --format shell` selected only `omegon`; `cargo fmt --all --check` passed; and `cargo clippy -p omegon --all-targets -- -D warnings` passed on the final source (`/tmp/omegon-project-clippy-direct.log`). This satisfies the same formatting/Clippy checks without claiming the stalled wrapper completed.

Implementation commit: `0fbee274`. No installed launcher was changed. The project browser is implemented in this checkout; interactive acceptance and the final full crate gate remain pending, as reflected in tasks.md.
