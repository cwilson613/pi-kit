# Native TUI usability repairs

## Intent

Fix three concrete findings from agent-operated native terminal trials: duplicate
and clipped permission choices, an inert Project search hint, and a clipped send
hint. The evidence is indexed in ../tui-project-shell/verification.md, sixth delivery.

## Scope

One authoritative set of permission choices with height-aware rendering; functional
Project browser filtering using existing menu state; width-budgeted composer help.
This increment retains existing runtime permission semantics and project navigation.
Persistent inline layout and execution/evidence drill-down remain in tui-project-shell.

## Success criteria

- Permission choices render once and remain readable at 50x18 and 90x30.
- Project search filters rows, supports editing and safe empty results, and preserves
  the draft and filter across inspection, refresh and covered permission prompts.
- The primary send/run hint remains visible at 40, 56 and 90 columns.
- Regression tests and rebuilt native screenshots verify the changes without operator input.
