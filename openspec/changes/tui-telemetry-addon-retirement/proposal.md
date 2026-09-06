# Retire decorative core telemetry; reserve an optional TUI addon

## Intent

The operator has marked the footer inference symbols and tool animations OBE
(overtaken by events). Their persistent space and maintenance cost no longer
justify inclusion in the core harness. This supersedes the core-product direction
of `tui-footer-engine-display` and the decorative instrument work in `tui-hud-redesign`.
Historical completion records remain history; they are not an obligation to retain
these widgets.

## Scope

Retire the inference and tools instrument panels, decorative engine glyphs,
waveforms, memory strings, recency animations, and their exclusive update state,
render paths, timers, tests, and documentation. Preserve useful domain telemetry
and expose actionable information as compact text or existing on-demand inspectors.
Audit shared effects individually so decision/error visibility and actual progress
remain understandable.

Record an optional first-class TUI addon as a future capability. Do not build an
addon SDK or keep the retired implementation behind a default-off core flag merely
to preserve the option. Git history remains the reference for an eventual addon.
The shared inline/fullscreen implementation continues under `tui-dual-presentation`.

## Success criteria

- Neither default TUI allocates inference/tools instrument rows or decorative engine symbols.
- Core rendering and scheduling no longer update state exclusively for those visuals.
- Model/context, active work, cancellation, permissions, and actionable failures remain discoverable.
- A future addon has a documented bounded rendering/event contract and explicit opt-in ownership.
- Agent-operated captures demonstrate reclaimed space and unchanged input/completion behavior.
