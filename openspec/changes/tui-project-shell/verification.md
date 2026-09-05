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

The fresh-session screen still reports an unavailable semantic projection (missing cursor or empty authority stream). The run establishes live turn completion and input recovery, not correct project/session projection initialization. That warning remains a task in this change.

## Checks

- Python fixture contract tests passed via `python3 scripts/tests/test_tui_acceptance.py`. The local interpreter lacks pytest; the file also supports direct execution.
- Focused TUI suite: 1,239 passed, zero failed, one ignored with canonical glyph environment. Controller and App replay regressions passed, alongside authoritative completion/idle queue and second-submission tests.
- `just clippy-changed`: passed for Omegon, all targets.
- Initial crate gate: 5,080 passed, one recovery campaign failed its 15-second wall-clock assertion under concurrent build load, ten ignored. No timing assertion changed. The complete `env -u NO_COLOR -u OMEGON_ASCII_GLYPHS RUST_TEST_THREADS=4 just test-crate omegon` rerun passed: 5,106 passed, zero failed, 11 ignored across nine suites, including the recovery campaign and all integration targets.
- OpenSpec structural validation passed; it does not imply completion of pending shell tasks.

Implementation commit: `b8ca3287` (replay repair, regression tests, terminal runner, and recipe). No installed launcher was changed.
