# Research and adoption decisions

Assessed 2026-09-05. Local implementation baseline:
`326ab36e9654f2d90e693e182ca9b0dd9d2d813e`, branch
`feature/tui-project-shell`. Neighboring implementation reference:
`../omegon` at `4aeee3b0e92bdf250af814ef4bdad1957ebf2dd7`.
The neighbor is implementation evidence, not product or specification authority.
Only the paths below were reviewed; this is not a complete upstream range audit.
No tracking marker, neighbor files, or Git history was changed.

## Library evidence

Cargo.lock resolves `ratatui` 0.30.0 and `ratatui-core` 0.1.0. The installed
registry source was inspected, including `terminal/terminal.rs`. Current online
documentation reports a later patch, so the pinned source controls this plan.

Ratatui already supplies Inline and Fullscreen viewports. Inline coordinates can
start below row zero. `insert_before` publishes above the live area, and draw
handles terminal-size changes. These provide the required rendering primitives.
[Pinned viewport documentation](https://docs.rs/ratatui/0.30.0/ratatui/enum.Viewport.html)
and [pinned Terminal API](https://docs.rs/ratatui-core/0.1.0/ratatui_core/terminal/struct.Terminal.html).

Pinned source observations: `insert_before` returns success without insertion on
non-inline viewports; callers must enforce ownership. It allocates a rectangular
buffer before insertion, so byte limits alone do not bound allocation. Its portable
implementation clears and redraws the live region. The optional `scrolling-regions`
implementation uses another path. The neighbor enables that feature; this checkout
does not request it directly. Keep the current dependency/features initially and
inspect Cargo feature unification when building. Adopt a feature change only after
comparative captures show a concrete benefit without compatibility regressions.

The viewport height is a construction option. Preserve an eight-row inline terminal
instead of reconstructing it for every draft-height change. Clip to terminal height;
scroll the shared editor within its assigned area. All layout roots use Frame::area.
These are local design choices, not requirements imposed by Ratatui.

## Selective adoption

| Evidence | Decision | Local application |
|---|---|---|
| Neighbor `tui/terminal_presentation_coordinator.rs` preserves an inline Terminal across fullscreen visits | Adopt the two-buffer ownership approach | Extend `tui/terminal_session.rs` and `terminal_presentation.rs`; retain one mode ledger and one I/O lock |
| Neighbor coordinator uses its own guard and rollback state | Skip direct import | Local success-ordered mode ledger already handles partial cleanup; avoid competing ownership |
| Neighbor `tui/render.rs::draw_exclusive_surface` introduces another overlay precedence chain | Skip | Existing `interaction.rs::navigation_owner` remains the visible/input authority |
| Neighbor `tui/transcript_publication.rs` peeks then commits publication | Adopt the intent | Extend local NativePublicationState's settlement rules; do not copy the pending text vector |
| Neighbor run loop collects/formats pending publications before insertion | Rework locally | Budget discovery, formatting, wrapping, and buffer cells before allocation |
| Neighbor `surfaces/inline.rs`, `tui/inline_render.rs` provide compact rows | Defer new abstraction | Extract shared composer/activity rendering first; introduce a row helper only for demonstrated duplication |
| Neighbor `examples/inline_viewport_probe.rs` | Adopt test technique | Use an isolated PTY/library test for cursor and scrollback behavior before App integration |
| Neighbor terminal-ownership document suggests retry/exactly-once behavior | Skip that guarantee | Retain local Committed/KnownFailure/Ambiguous outcomes; uncertain physical writes cannot be safely replayed |

Both workspace manifests declare BUSL-1.1. Adaptation stays within this project;
retain source provenance and applicable notices when copying any implementation.
No third-party harness source or new dependency is proposed.

## Local findings that change implementation order

1. `scripts/omegon-launcher.sh` resolves both entry names to the same binary and
   executes that path. Basename intent must cross this boundary explicitly.
2. `settings.rs::Profile::capture_from` currently elides the Om default and captures
   other effective values. New launch-dependent defaults require explicitness
   tracking so an unrelated profile save does not persist an inferred preference.
3. `surfaces/layout.rs` separates detail dimensions but Full also enables all
   surface flags. Layout eligibility must be separate from requested visibility.
4. `tui/render.rs::App::draw` refreshes dashboard state, counters, picker state,
   and update notifications. Move shared preparation before layout selection;
   do not skip these updates while an exclusive view is visible.
5. `TerminalSessionHandle::with_fullscreen_io` currently requires alternate-screen
   ownership. Extend the guarded operation to validate the actual expected surface;
   do not relax it to unrestricted stdout access.
6. `print_transcript_to_native_scrollback` builds the complete export, and
   NativePublicationState hashes its committed prefix before starting the preparation
   timer. This explicit-export path is not an incremental automatic publisher.
7. `conversation_projection.rs` can synthesize turn/tool outcomes and clones complete
   projections. Publication must wait for finalized groups and prepare bounded ranges.
   Segment has no universal stable ID: use an attachment/generation plus canonical
   range and within-record cursor, not a fabricated durable protocol identifier.
8. `run_tui` unconditionally enters alternate screen, sets a global background, and
   clears the screen before drawing. Inline startup must bypass those operations;
   a viewport option alone cannot preserve the existing primary screen. Its early
   image-protocol query can be deferred until rich image inspection is needed.

## Remaining empirical questions

- Does portable insertion produce objectionable flicker in the five native clients?
  Measure first; this does not block implementation or justify a terminal fork.
- Do primary-buffer cursor restoration and resize differ across clients? Resolve
  through the first captured transition slice, before changing launch defaults.
- The previous rebuilt executable and broad gates encountered macOS pre-entry
  `_dyld_start` stalls. This is a recorded verification risk, not a demonstrated
  renderer defect. Recheck once against the intended artifact during implementation;
  if it recurs, report the blocked evidence gate without repeating cold builds.

No unresolved product or architectural decision prevents beginning the test-first
implementation. Empirical acceptance gates remain mandatory.
