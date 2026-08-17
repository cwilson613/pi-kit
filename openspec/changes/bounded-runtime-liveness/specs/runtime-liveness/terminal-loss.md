# Runtime liveness: terminal loss — Delta Spec

## ADDED Requirements

### Requirement: Terminal loss is an authoritative runtime boundary

A directly attached interactive runtime must treat terminal input boundary loss as a supervisor-owned lifecycle transition rather than an ordinary queued command.

#### Scenario: Active turn is revoked on terminal loss
Given an interactive runtime has an active generation-scoped turn
And the terminal input boundary reports permanent loss
When the coordinator handles the boundary
Then the supervisor admits exactly one revoked outcome for that turn
And no later assistant completion or tool admission is published for the revoked identity

#### Scenario: Idle session exits after terminal loss
Given an interactive runtime has no active turn
And the terminal attachment is permanently lost
When bounded teardown runs
Then session tasks receive cancellation
And retained state persistence is attempted
And the runtime reaches a terminal session outcome without requiring another input or draw event

### Requirement: Liveness boundaries are observable

The runtime must expose monotonic evidence for terminal acquisition, supervisor admission, cancellation, child termination, settlement, and session teardown.

#### Scenario: Stalled boundary is attributable
Given a terminal-loss or cancellation sequence has started
When a later boundary does not complete within its budget
Then diagnostics identify the sequence identity and last completed boundary
And repeated unchanged polling is not recorded as semantic progress
