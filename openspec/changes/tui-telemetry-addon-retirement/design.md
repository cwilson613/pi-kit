# Retirement design

## Decision and sequencing

Operator decision: decorative inference/tool footer telemetry is OBE. Land this
retirement after the current shared-layout slice, using its two default-entry
captures as the before evidence. The retirement must delete exclusive core code;
merely hiding panels would recover space but not maintenance cost.

## Existing owners to audit

- `tui/instruments.rs`: inference/tools rendering, wave/glyph/recency state and tests.
- `tui/mod.rs::render_bottom_footer`: allocation and fallback composition.
- `tui/render.rs`: shared preparation, instrument projection/update, effect processing.
- `tui/agent_events.rs`, `tui/effects.rs`, `tui/frame_scheduler.rs`: telemetry-driven updates and animation demand; confirm the scheduler's actual owner before editing.
- `surfaces/instruments.rs`: renderer-neutral facts; retain fields with genuine runtime or client consumers.
- `surfaces/layout.rs`, `tui/footer.rs`, UI menus, profile compatibility and snapshots.

Classify each datum as operational or visualization-only by its remaining callers.
Preserve model selection/context pressure and active operation/decision state in
compact text. Remove tier/provider symbol chains and ornamental motion from the
core footer. This is a component retirement, not a general ban on status indicators
or an unsolicited redesign of the Project/Workbench surfaces.

## Configuration migration

Active and Full remain evidence preferences. Neither mounts retired instruments.
Old `/ui show instruments` and stored visibility requests must report retirement
and the future optional-addon direction rather than silently pretending to enable
something. Remove dead menu toggles and documentation. Do not write inferred
replacement profile values during startup.

## Future addon direction

Use the existing extension/plugin admission and lifecycle as the starting point.
A future proposal must establish a TUI rendering contribution contract with bounded
semantic inputs, invalidation demand, allocated rectangles, capability metadata,
resource limits, and revocation. The host owns Ratatui, terminal buffers and input.
Do not expose `App`, terminal writers, or arbitrary drawing outside the allocation.
Do not introduce an in-process ABI or public SDK until a concrete addon is pursued.
The archived implementation and Git history preserve reusable algorithms.

## Validation

Red-first tests should assert reclaimed geometry for both bases/details, compact
status at 40/56/90 columns, no decorative animation demand at idle, and truthful
legacy-command behavior. Retain permission, draft, cancellation and second-turn
regressions. Run crate/Clippy/script gates and attributable PTY/native before/after
captures. A smaller screenshot alone is not evidence that dead code was removed.
