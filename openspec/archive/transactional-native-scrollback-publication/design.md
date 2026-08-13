# Transactional native scrollback publication — Design

## Context

The TUI currently publishes native scrollback as one prepared transcript followed by one terminal insertion. A global byte ceiling prevents an unbounded write, but it cannot resume oversized canonical content or distinguish prepared, committed, failed, and ambiguously delivered ranges.

## Design

Introduce a TUI-local `NativePublicationState` owned by the presentation task. Canonical transcript text remains authoritative.

Each prepared chunk carries:

- terminal attachment epoch;
- canonical base and target revisions;
- UTF-8 byte range;
- preparation timestamp and bounded row/record metadata.

Preparation peeks from the committed cursor and applies byte, row, record, and elapsed-time budgets. Insertion success commits exactly the prepared contiguous range. Known failure preserves the cursor. Ambiguous delivery marks the attachment degraded and prohibits blind retry; recovery uses a bounded snapshot rebuild or managed viewport.

The state machine does not claim exactly-once physical terminal bytes. It guarantees canonical losslessness, contiguous logical commits, and no intentional duplicate append after ambiguous delivery.

## Initial file scope

- `core/crates/omegon/src/tui/native_publication.rs` — range state machine and focused tests
- `core/crates/omegon/src/tui/mod.rs` — presentation-owner integration
- `core/crates/omegon/src/tui/terminal_session.rs` — attachment epoch only if existing session identity cannot supply it

## Validation

- focused state-machine tests for commit/failure/stale/noncontiguous/Unicode chunking
- existing TUI publication tests
- `just clippy-changed`
