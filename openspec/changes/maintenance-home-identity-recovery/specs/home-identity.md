# Installation home identity recovery

## ADDED Requirements

### Requirement: Recovery preserves installation authority

An explicit maintenance recovery operation preserves existing policy and audit
evidence and binds a new home identity only after continuity is established.

#### Scenario: Unproven continuity
Given an installation record that does not match the opened home identity
When continuity evidence is insufficient
Then recovery refuses the rebind and leaves existing authority intact

#### Scenario: Interrupted recovery
Given an admitted recovery transaction with a preserved original record
When the process stops before settlement
Then the next maintenance invocation can identify the incomplete transaction
And ordinary admission cannot use an ambiguously rebound authority

#### Scenario: Policy survives recovery
Given proven continuity and existing contribution deny records
When the recovery transaction settles
Then the applicable deny policy and audit history remain enforceable
