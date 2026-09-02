# runtime-session/authority - Delta Spec

## ADDED Requirements

### Requirement: Cross-surface semantic parity is executable

TUI, ACP, Web, IPC, CLI, and daemon adapters must be tested against shared
authoritative snapshots and action descriptors. The executable matrix must prove
that differences are limited to declared transport support, serialization, and
redaction rather than duplicated semantic, availability, or admission policy.

#### Scenario: Shared edge fixture is projected
Given one authoritative session snapshot and canonical action set
When every supported edge adapter projects the fixture
Then semantic state and action availability agree across all representable fields
And each difference names a declared transport limitation or compatibility rule

#### Scenario: Activity observations have an authority order
Given durable session state and a cached runtime queue observation refer to the same session and generation
When an adapter reconciles their activity revisions
Then the newer revision determines queue, active-turn, and terminal state
And an unversioned or older active observation cannot override a newer durable closure

#### Scenario: One-shot CLI projects shared semantics
Given the shared fixture contains persistent activity and canonical actions
When the one-shot CLI adapter projects it
Then representable identity, terminal, action, owner, and denial fields match the shared projection
And persistent busy reconciliation is explicitly declared unsupported rather than fabricated

#### Scenario: One edge misses terminal advice
Given an adapter misses an advisory terminal event after authoritative turn closure
When the next shared snapshot is projected
Then that adapter clears its active gate and admits a subsequent prompt
And no adapter-specific terminal policy is required

#### Scenario: Delayed terminal advice arrives after reconciliation
Given a persistent adapter reconciled to a newer idle activity revision and admitted another prompt
When delayed or duplicate terminal advice for the prior turn arrives
Then the adapter does not clear or corrupt the newer turn state
And terminal settlement remains exactly once

### Requirement: Bounded execution enforces admitted task policy prospectively

A bounded task manifest must become immutable admitted execution policy before
provider or tool dispatch. Time, turn, token, and tool limits must be checked
before the next governed action, and exhaustion must produce a typed structured
outcome after owned authority settles.

#### Scenario: Next provider request exceeds the token budget
Given a bounded task has consumed the admitted token budget
When the loop proposes another provider request
Then dispatch is refused before network activity
And the task settles as exhausted with observed and admitted budget evidence

#### Scenario: Task manifest is invalid
Given a task manifest has an unsupported or contradictory execution limit
When bounded execution admission evaluates it
Then execution fails before session, route, provider, tool, or child-process authority starts
And the structured result identifies the invalid manifest field

#### Scenario: Admitted task reaches its tool budget
Given a bounded task manifest admits a tool budget before session authority starts
And model execution consumes the exact admitted number of native tool calls
When the model proposes one more tool call
Then invocation preparation, lease creation, and native RPC are refused before owner entry
And structured exhaustion reports admitted and observed tool calls after owned authority settles
