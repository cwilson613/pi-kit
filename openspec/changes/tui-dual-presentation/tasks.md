# Implementation tasks

All tasks are intentionally unchecked: this change is planned, not implemented.
For each group, first run the proposed regression against existing behavior and
record the expected assertion failure. A compile error is not the red evidence.
Names below are proposed test names, not claims that these tests already exist.

## 1. Resolve the two independent preferences
<!-- specs: tui-presentation -->

- [ ] Add failing `entry_defaults_resolve_independently`, `explicit_ui_overrides_win`, `unrelated_profile_save_preserves_absent_ui_preferences`, `explicit_default_detail_is_persisted`, and `legacy_density_maps_to_active` tests in main/settings/layout owners.
- [ ] Add launcher regressions in `scripts/tests/test_omegon_launcher.py` for basename handoff, stale inherited marker, exact literal arguments, direct binary fallback, maintenance, --which, and headless/help/version behavior.
- [ ] Implement the pure resolution/explicitness model and opt-in `--tui`/`--ui` flags in `main.rs`, `settings.rs`, `surfaces/layout.rs`, and TuiConfig. Preserve existing launch defaults until task group 7.
- [ ] Extend existing UI routing/reporting for independent axes and session-local base switching; maintain surface preferences independently from layout eligibility. Add `ui_full_does_not_enter_alternate_screen` and `base_change_waits_for_mounted_root` tests.
- [ ] Run focused settings/layout/CLI/launcher checks and record red/green evidence.

## 2. Prove terminal ownership before App integration
<!-- specs: tui-presentation, tui-inline-publication -->

- [ ] Add failing coordinator tests: `inline_start_preserves_primary_history`, `inactive_terminal_cannot_write`, `covered_root_retains_fullscreen`, `resize_precedes_inline_insert`, and `fullscreen_insert_cannot_commit`.
- [ ] Add fault-injection tests for each acquisition/release, failed terminal creation, failed rollback, failed geometry restoration, and subsequent cleanup. Assert actual operation order and ledger state, not only the final presentation enum.
- [ ] Extend `terminal_session.rs` and `terminal_presentation.rs` with active-surface validation and coordinated Terminal buffers, retaining the current mode ledger, lock, and emergency restoration path.
- [ ] Add `inline_startup_never_flashes_fullscreen` and output-attribute restoration regressions; bypass unconditional alternate-screen clearing/splash and defer image probing for inline startup. Keep live diagnostic writes within existing notification/logging ownership.
- [ ] Add an isolated PTY contract using the locked Ratatui backend: preexisting primary text, eight-row nonzero origin, insert_before, alternate round trip, resize, and exit. Verify primary history separately from the current screen.
- [ ] Verify mouse-disabled and mouse-enabled fullscreen preferences with native mouse ownership in inline. Keep dependency versions/features unchanged for the first captured slice.

## 3. Reuse rendering and navigation
<!-- specs: tui-presentation -->

- [ ] Capture existing fullscreen component expectations, then add failing `inline_composer_respects_nonzero_origin`, `multiline_draft_survives_layout_switch`, `shared_preparation_runs_under_inspector`, and `inline_decision_reserves_all_actions` tests.
- [ ] Extract shared frame preparation and composer/activity/decision composition from `tui/render.rs`; wire two small layouts without another editor, App, or navigation precedence chain.
- [ ] Reuse Project, menu, panel, inspector, extension, and tutorial components; derive temporary fullscreen requirements from mounted interaction state, including covered roots and undersized decisions.
- [ ] Add `permission_round_trip_preserves_filtered_project`, `queued_decisions_keep_root_mounted`, and `paste_never_reaches_covered_composer` tests parameterized over both bases.
- [ ] Retain and run completion regressions for supervisor_completed without AgentEnd, idle-queue recovery, and second-turn submission in both presentations. Verify cancellation during browsing preserves the draft.
- [ ] Verify 40/56/90-column and short-height geometry, wide/combining Unicode, autocomplete, text image references, and scope-correct permission labels using shared widget tests.

## 4. Make publication incremental and bounded
<!-- specs: tui-inline-publication -->

- [ ] Add failing tests for prompt-once, finalized-group order, no partial-response publication, cancellation/failure outcomes, resume boundary, and fullscreen-first backlog in `native_publication.rs` and relevant projection owners.
- [ ] Add deterministic budget tests using injected time/work accounting: `large_history_does_not_scan_committed_prefix`, `oversized_record_is_chunked_before_formatting`, `wrapped_rows_and_cells_bound_insert`, and `zero_geometry_does_not_advance`.
- [ ] Extend the existing publication owner with generation/range/within-record cursors and bounded projection preparation. Add mutation-owner invalidation for session replacement and compaction. Do not add a duplicate transcript or durable segment-ID protocol.
- [ ] Integrate one automatic batch after input/lifecycle admission per cycle, bounded live previews during streaming, and progress indication during backlog. Guard insertion and flush through the active terminal owner.
- [ ] Add `write_failure_degrades_without_replay`, `known_nonwrite_preserves_cursor`, `stale_generation_cannot_settle`, and `resize_detail_change_preserves_partial_record` fault/state regressions.
- [ ] Separate explicit snapshot-export bookkeeping from automatic delivery within the existing publication owner; verify labeled export, geometry restoration, preserved cursor, and bounded normal-exit draining.
- [ ] Verify escaped/untrusted control content follows existing safe display/export treatment and cannot inject terminal mode commands through the new publication path.

## 5. Capture the first complete interaction
<!-- specs: tui-presentation, tui-inline-publication -->

- [ ] Add failing fixture/observation contracts before extending `scripts/tui_acceptance.py`: explicit surface flags, deterministic stream/tool gates, distinct historical/current-view markers, and scenario-specific request counts.
- [ ] Automate inline startup with shell-history marker, submit/stream, preserved next draft, F2 filtered detail, permission denial above Project, return inline, second successful turn, resize, and clean exit.
- [ ] Add a large-output/background scenario with deterministic cancellation and generation replacement; assert input admission without draining the whole backlog first.
- [ ] Build once through the repository workflow, freeze the binary/runner with source and hash provenance, run the first PTY and native trial, and inspect the actual captures. Record failures and do not substitute older screenshots.
- [ ] Resolve demonstrated geometry, priority, or duplication failures before enabling entry defaults. If portable insertion flickers, compare scrolling-regions in a separately identified build and adopt it only with evidence.

## 6. Native compatibility and recovery acceptance
<!-- specs: tui-presentation, tui-inline-publication -->

- [ ] Extend `scripts/tui_native_acceptance.py`, operator-kit arguments, and their tests to select both presentations and record active buffer, current view, and history observations independently.
- [ ] Run the acceptance matrix in verification.md using owned Ghostty, iTerm2, kitty, WezTerm, and Apple Terminal windows, automated input, screenshot inspection, fixture outcomes, and process-tree cleanup.
- [ ] Exercise startup near the terminal bottom, repeated alternate visits, width/height changes, multiline paste, cancellation, explicit export, shell handoff, normal exit, and catchable termination at their appropriate test layers.
- [ ] Classify unsupported native controls as unverified cells and use the PTY harness for those controls; never claim a native key/paste check that the adapter cannot perform.

## 7. Enable defaults, document migration, and land
<!-- specs: tui-presentation, tui-inline-publication -->

- [ ] Enable tested om inline/Active and omegon fullscreen/Full defaults and the installed launcher marker. Re-run entry-point acceptance through fixed-build wrappers rather than only direct --tui flags.
- [ ] Update `pkl/Profile.pkl`, UI help/settings menus, applicable public CLI/config docs, root TUI directives, and CHANGELOG Unreleased for shipped behavior and compatibility aliases.
- [ ] Run serialized omegon crate tests, `just clippy-changed`, focused script tests, and `just test-dev-scripts`; broaden only if implementation touches shared crates/contracts. Complete required schema/document checks for changed surfaces.
- [ ] Run OpenSpec validation, reconcile this change's evidence and parent pending tasks, inspect the final diff, and create logical commits. Record exact incomplete gates without checking their tasks.
- [ ] Verify every scenario against implementation and recorded evidence. Leave archival for actual completion and the intended lifecycle close; do not close the parent reconstruction's unrelated pending work.
