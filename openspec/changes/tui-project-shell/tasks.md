## 1. Captured terminal foundation
<!-- specs: tui-project-client -->

- [x] Add and verify local streaming fixture contract tests before the runner implementation.
- [x] Automate actual terminal launch, two prompts, resize, isolated state, and evidence capture.
- [x] Run the scenario interactively, inspect captures, and record build provenance and limitations.

- [x] Reproduce and repair deferred completion replay that prevented second-turn input; verify controller and App regressions.

## 2. Client interaction ownership
<!-- specs: tui-project-client -->

- [x] Initialize and validate fresh-session projections before launching interactive clients; render an explicit empty-session ready state.
- [x] Add regression scenarios for visible approval/input agreement while browsing and return to prior selection.
- [x] Introduce responder-backed decision ownership with arrival ordering, bounded overflow, preserved passive state, and matching prompt/input precedence.
- [x] Propagate profile permission policy into runtime settings; capture a real denied write above Settings and return to the prior surface.
- [ ] Extend shared ownership to all passive navigation and extension interactions.
- [ ] Adapt terminal ownership from the inline corpus with all terminal mode changes routed through one owner.

## 3. Project and work vertical slice
<!-- specs: tui-project-client -->

- [ ] Specify project/session/work composition using existing semantic and runtime read models before implementation.
- [ ] Implement project → session/work → execution/evidence → decision/cancel → conversation navigation.
- [ ] Extend captured acceptance for recovery, queued decisions, cancellation, and preserved drafts/selections.
