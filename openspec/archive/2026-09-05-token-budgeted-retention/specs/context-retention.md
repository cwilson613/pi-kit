# Context retention delta

## ADDED Requirements

### Requirement: Compaction budgets retained complete turns

Compaction uses the effective assembly budget with reply, schema, system-context,
and summary headroom reserved. It selects a suffix of complete numeric agent turns
within the existing age window. An absent token budget preserves legacy planning.
The target is an estimate, not a guarantee about provider tokenization or summary size.

#### Scenario: Oversized recent history
Given large messages from several recent turns exceed the retained token target
When pressure, overflow, or manual compaction is planned
Then older complete turns are selected for summary even if inside the age window
And the retained suffix fits the estimated target unless its protected group exceeds it

#### Scenario: Protected group exceeds target
Given the newest populated recent turn or a tool exchange connected to it exceeds the target
When compaction is planned
Then the protected group remains intact
And a returned plan reports the over-budget exception
And no message content is truncated to satisfy the target

#### Scenario: Tool exchange crosses a candidate boundary
Given a tool call and its result have different turn numbers
When retention chooses an eviction boundary
Then both sides of that exchange remain on the same side of the boundary

#### Scenario: Effective context policy
Given a requested assembly class smaller than provider capacity
When the retained-context target is computed
Then the smaller assembly window and existing reply and schema reserves are used
And system-context and summary headroom are subtracted without arithmetic underflow

### Requirement: Application and summary preserve planned context

All production compaction callers apply the selected window. Prior summary context
is included in subsequent summary input. The newest populated agent turn within the primary age window is protected;
older user requests can enter the summary during long autonomous tool runs.

#### Scenario: Apply a reduced retention window
Given the planner selects fewer recent turns than the previous fixed manual window
When compaction succeeds through a loop or manual caller
Then exactly the planned messages are evicted
And retained tool exchanges remain complete

#### Scenario: Repeated compaction
Given a conversation already has a summary and more old messages
When another compaction payload is built
Then it includes the previous summary and the newly selected messages

#### Scenario: Cancelled planning
Given planning has been cancelled
When a plan is requested
Then no applicable compaction plan is returned

### Requirement: Durable compaction uses current aligned context

Before durable compaction starts, its current authoritative message sequence must
align with the planner's canonical sources and prior summary. The input and
retained manifests use current semantic source identities. Existing request-based
retained records remain readable.

#### Scenario: Results arrived after the last request
Given current authoritative context includes results after the last prepared request
When an aligned compaction is committed
Then those results appear in the input or retained manifest according to the plan
And reopening the authority reproduces the retained messages and new summary

#### Scenario: Incompatible local and durable projections
Given canonical source messages do not align with current authoritative context
When compaction admission is attempted
Then admission fails before compaction mutation or provider dispatch
And existing local messages remain intact

#### Scenario: Prior summary shifts context item counts
Given a previous summary precedes canonical messages in authoritative context
When another compaction is admitted
Then the previous summary is included in summary input separately from canonical eviction count
And the durable retained boundary matches the local plan

#### Scenario: Nonprefix restored turn order
Given restored messages place an older turn after a newer retained turn
When the budgeted planner selects a boundary
Then it widens retention until the selected messages form a chronological suffix
And durable admission rejects any legacy plan whose eviction is not a prefix

#### Scenario: Legacy or mixed lineage
Given a durable session does not have full semantic lineage
When compaction admission is attempted
Then admission reports the unsupported alignment before mutation
And old request-based compaction records remain readable
