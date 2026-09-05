# Project TUI client - Delta Spec

## ADDED Requirements

### Requirement: Captured deterministic terminal acceptance

The repository provides an automated terminal scenario using the real executable, isolated local provider and user state, bounded waits, and attributable screen captures.

#### Scenario: Two turns and resize
Given a freshly built executable and a local streaming provider fixture
When the runner launches the TUI and submits two prompts through terminal input
Then both distinct fixture replies appear in captured terminal screens
And the second reply remains visible after resizing
And the evidence identifies the binary, source, process, dimensions, and capture hashes

### Requirement: Unified client interaction ownership

The reconstructed client uses one visible interaction owner for keyboard dispatch, with explicit return targets and stable domain identities.

#### Scenario: Approval during project browsing
Given the operator is browsing project work
When a runtime approval arrives
Then the visible interaction and keyboard owner agree
And resolving the approval returns to the prior stable work selection

### Requirement: Drawn event replay makes progress

Events released after a stream draw must not requeue behind their successors. Runtime lifecycle and queue authority events must bypass presentation buffering entirely.

#### Scenario: Completion backlog
Given a stream chunk followed by multiple completion events before the next draw
When the TUI acknowledges that draw
Then the completion backlog drains in order
And terminal input can submit a second turn
