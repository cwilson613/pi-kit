# OpenCode2 parity - Delta Spec

## ADDED Requirements

### Requirement: Parity evidence distinguishes source and executable behavior

The parity campaign must record reference identity, local build identity,
scenario inputs, observable results, and unresolved evidence differences.

#### Scenario: Beta differs from documentation
Given a pinned beta executable and a documented behavior
When the reference fixture produces a different result
Then the campaign records the discrepancy and executable identity
And it does not mark the local behavior deficient solely from documentation

### Requirement: Scoped instructions are admitted with durable provenance

The harness must combine applicable ancestor directives and admit instruction
changes before dispatch while preserving scope, authority, and replay identity.

#### Scenario: Intermediate ancestor and long root policy
Given root and intermediate AGENTS.md files and a nested working directory
And the root guidance exceeds 4000 bytes
And the applicable guidance fits the configured instruction budget
When the harness prepares the model request
Then both applicable files are represented without silent truncation

#### Scenario: Required instructions exceed budget
Given applicable required guidance exceeds the configured instruction budget
When the harness prepares the model request
Then it prevents dispatch with an actionable budget diagnostic
And it does not silently drop required guidance

#### Scenario: Instruction changes and temporary read failure
Given an admitted instruction generation
When the next preparation observes changed guidance and a temporarily unavailable source
Then it admits the changed available guidance and retains the unavailable source's last admitted value
And replay reconstructs the admitted values without reading current files

#### Scenario: Initial source unavailable
Given no admitted instruction generation and an unavailable required source
When the harness prepares a model request
Then dispatch waits for a successful observation or explicit cancellation
And the operator can identify the unavailable source

#### Scenario: Confirmed deletion and unchanged reread
Given an admitted source that is later confirmed absent
When the harness prepares the next model request
Then it records removal of that source
And later observations of the same absence append no duplicate change

### Requirement: MCP deadlines distinguish operation phases

MCP must enforce separate startup, catalog, and execution budgets with compatible
legacy fallback and truthful cancellation settlement.

#### Scenario: Slow execution with fast discovery
Given a server whose startup and catalog finish within their configured budgets
And an execution budget longer than its catalog budget
When a tool completes after the catalog budget but before the execution deadline
Then the tool result succeeds
And startup and catalog budgets do not prematurely terminate execution

#### Scenario: Legacy configuration and cancellation
Given a server configured only with timeout_secs
When an MCP operation is cancelled before its inherited deadline
Then cancellation settles the operation without waiting for the timeout
And local descendants are terminated or cleanup failure is reported
And remote termination is not claimed without evidence

### Requirement: Compaction retains complete context within a token budget

Compaction must retain the newest complete conversation units that fit its
budget and commit replacement only after successful summary validation.

#### Scenario: One oversized recent turn
Given a recent turn exceeds the retention budget and includes a tool transaction
When the planner selects retained context
Then it does not split the tool call from its result
And it selects a bounded complete suffix or reports that required context cannot fit

#### Scenario: Summary fails during instruction change
Given a compaction bound to an admitted instruction generation
When summary generation fails
Then the active context revision remains unchanged
And no new instruction generation is inferred from the failed summary

### Requirement: Model presets resolve through offering admission

Named model presets must resolve against the selected offering and its inventory
generation before changing the active route or dispatching a request.

#### Scenario: Unknown or unsupported preset
Given an active route and a requested preset absent from the selected offering
When the operator selects that preset through TUI, CLI, or ACP
Then selection fails without dispatch or route replacement
And all surfaces expose the same semantic failure

#### Scenario: Valid preset with stale inventory
Given a preset selected from an inventory generation that has been replaced
When the route is admitted for execution
Then current offering and control evidence are revalidated under existing lease policy
And stale evidence cannot authorize an unsupported request

### Requirement: Continuity findings are reconciled against existing owners

The campaign must exercise client and child lifecycle behavior before proposing
replacement infrastructure. Confirmed defects must have bounded fixes or deferrals.

#### Scenario: Reconnect with pending work
Given pending input, an approval request, and a background delegate on an existing server
When a client disconnects and reconnects
Then the campaign compares recovered semantic identities and results
And it records any lost or duplicate input, approval, or completion as a specific finding

#### Scenario: Ambiguous execution across restart
Given an external tool may have acted before the server stopped
When the recovery fixture restarts the server
Then the campaign verifies whether execution remains uncertain without an unsafe automatic rerun
And it records the durable evidence and any violated local recovery contract
