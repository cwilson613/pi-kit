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
- [x] Extend shared visible/input ownership to passive navigation and extension overlays.
- [x] Adapt success-ordered terminal ownership from the inline corpus; route current fullscreen/native-export mode changes through one owner.
- [ ] Add responder transport and provenance for extension actions; the client currently reports unsupported responses.
- [ ] Implement the persistent inline viewport and bounded automatic transcript publication, with backlog/recovery acceptance.

## 3. Project and work vertical slice
<!-- specs: tui-project-client -->

- [x] Specify the first project/session/work composition using existing session inventory and Workbench read models.
- [x] Implement F2 Sessions/Work browsing, item inspection, stable refresh, draft preservation, approval return, and explicit idle resume; verify focused TUI regressions.
- [ ] Complete the final crate gate and capture the F2 browser scenario after the host executable-loader stall is resolved.
- [ ] Implement project → session/work → execution/evidence → decision/cancel → conversation navigation.
- [ ] Extend captured acceptance for recovery, queued decisions, cancellation, and preserved drafts/selections.
