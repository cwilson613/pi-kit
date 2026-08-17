# Runtime liveness: process terminalization — Delta Spec

## ADDED Requirements

### Requirement: Owned process groups terminate under bounded stages

Every ordinary Bash execution must have finite cancellation and reap ownership, including when no operator timeout was supplied.

#### Scenario: Cancellation escalates and stops awaiting reap
Given an owned process group does not exit after cancellation
When the TERM grace period expires
Then the runtime sends KILL to that exact process group
And waits only for the configured reap budget
And records an indeterminate terminalization fault if reap still does not complete

#### Scenario: Tool settlement occurs exactly once
Given cancellation races with process exit and output-pump completion
When the process terminalization state machine resolves
Then exactly one terminal tool outcome is published
And later exit, EOF, or timeout observations cannot publish another outcome

#### Scenario: Missing explicit timeout uses a finite runtime default
Given an ordinary Bash execution does not specify timeout_secs
When the command remains nonterminal
Then the runtime applies a finite default absolute deadline
And expiry begins owned process-group terminalization
