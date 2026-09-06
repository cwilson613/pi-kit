# Inherited TUI review corrections delta

## ADDED Requirements

### Requirement: Authoritative completion releases abandoned runtime decisions

Matching authoritative terminal or idle state and session reset must resolve active
and queued runtime-owned decisions negatively and release their input ownership.
UI-local draft and covered menu state must survive this cleanup.

#### Scenario: Active and queued decisions outlive their turn
Given a current runtime turn with an active permission request and a queued operator wait request
When matching authoritative terminal state or an idle runtime snapshot arrives
Then permission resolves to Deny and the operator wait resolves to Cancelled
And no deferred decision can reappear after cleanup
And the preserved composer can submit the next prompt after the operator closes its covered menu

#### Scenario: Session reset abandons queued decisions
Given active and queued runtime decisions over an unsent draft
When the session resets
Then their response channels resolve negatively and decision input ownership ends
And the draft remains available

#### Scenario: Advisory or stale completion
Given an active decision owned by the current runtime turn
When an advisory turn-end event or an older turn's terminal event arrives
Then that event does not resolve or discard the current decision

#### Scenario: Timed-out wait has a queued successor
Given an active operator wait with a tool-call identity and a queued permission request
When ToolEnd arrives for that wait's tool-call identity
Then the expired wait releases input and the queued permission becomes actionable

#### Scenario: Old wait completion cannot dismiss a newer wait
Given an active operator wait with a newer tool-call identity
When ToolEnd arrives for another wait or a wait without matching identity
Then the current wait and its responder remain active

### Requirement: Native resize cleanup relinquishes only removed panes

Native trial cleanup must release resize-pane ownership after successful removal
and close any remaining resize pane before closing the main pane.

#### Scenario: Successful resize restoration
Given a WezTerm trial that created a temporary resize pane
When the trial restores its geometry and later closes the main pane
Then cleanup does not attempt to remove the already-removed resize pane
And disappearance of the private GUI socket after main-pane closure is not a failed second removal

#### Scenario: Resize pane removal fails
Given an owned resize pane
When its removal fails
Then cleanup retains its ownership for retry
And the trial does not claim successful window cleanup
