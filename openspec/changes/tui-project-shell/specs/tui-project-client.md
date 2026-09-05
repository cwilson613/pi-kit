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

### Requirement: Initial semantic view precedes client launch

Session-backed interactive startup creates and validates its initial authority-derived projections before launching TUI or IPC consumers. It preserves recorded lineage rather than inventing semantic history for an empty session.

#### Scenario: Fresh session without projection caches
Given a fresh session has no background projection cache
When interactive startup initializes the session
Then the first screen reads a validated view of the created authority stream
And startup does not display a missing-cursor or empty-authority warning

### Requirement: Responder-backed decisions share visible and keyboard ownership

Permission and manual-action requests serialize in arrival order above passive surfaces. The queue is bounded and overflow resolves negatively.

#### Scenario: Multiple decisions while a passive surface is open
Given a permission request owns input above a Settings or copy surface
When a manual-action request arrives
Then it waits until the permission is resolved
And its prompt becomes visible when it becomes the keyboard owner
And resolving both requests preserves the prior passive surface state

#### Scenario: Decision queue capacity
Given one active decision and 64 queued decisions
When another permission request arrives
Then the new request is denied explicitly
And the active decision and existing queue remain intact

### Requirement: Profile tool permissions reach invocation policy

Applying a profile replaces the runtime permission policy with the profile's declared policy.

#### Scenario: Prompt rule in the active profile
Given the isolated profile declares write as prompt
When a model requests the write tool
Then a permission prompt is visible and receives operator input
And denying the prompt prevents the write
