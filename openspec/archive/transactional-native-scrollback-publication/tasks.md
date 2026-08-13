# Tasks

Dependencies: Group 2 depends on Group 1. Group 3 depends on Groups 1 and 2.

## 1. Native publication state machine
<!-- specs: tui-presentation -->

- [x] 1.1 Add failing tests for bounded UTF-8 chunk preparation, contiguous commit, known failure, stale epoch, and ambiguous delivery.
- [x] 1.2 Implement attachment/revision/range identities and checked state transitions in `core/crates/omegon/src/tui/native_publication.rs`.
- [x] 1.3 Enforce byte, record, visual-row, and elapsed-time preparation budgets without advancing the committed cursor.

## 2. Presentation-owner integration
<!-- specs: tui-presentation -->

- [x] 2.1 Replace whole-transcript native insertion in `core/crates/omegon/src/tui/mod.rs` with prepare/insert/commit orchestration.
- [x] 2.2 Preserve the canonical cursor after known insertion failure and disable blind retries after ambiguous delivery.
- [x] 2.3 Invalidate stale publication work across attachment/session boundaries and expose managed-viewport degradation.

## 3. Verification and lifecycle reconciliation
<!-- specs: tui-presentation -->

- [x] 3.1 Run focused state-machine and TUI publication tests.
- [x] 3.2 Run `just clippy-changed` and `git diff --check`.
- [x] 3.3 Reconcile task status, assess all scenarios, update the bound design node, and commit the implementation.
