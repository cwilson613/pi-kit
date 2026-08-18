# Runtime capability declarations

## Requirements

### Requirement: Runtime contributions have stable declarations

The runtime must project registered tools and built-in operator commands into renderer-neutral capability declarations with stable identifiers, kinds, owners, and invocation bindings without changing their existing execution authority.

#### Scenario: Tool declaration preserves invocation ownership
Given an existing model tool definition registered by a feature
When the declaration inventory is built
Then it contains one tool capability with a stable identifier
And its invocation binding names the tool and owning feature

#### Scenario: Command declaration preserves canonical identity
Given an existing built-in command definition
When the declaration inventory is built
Then it contains one operator-action capability
And aliases remain invocation bindings to that canonical capability rather than independent capabilities

### Requirement: Registry integrity is validated before authority migration

The declaration inventory must reject ambiguous or structurally invalid registry state with typed diagnostics, including duplicate capability identities, ambiguous invocation bindings, missing owners, and dangling group members.

#### Scenario: Duplicate capability identity is rejected
Given two declarations use the same capability identifier
When registry integrity is validated
Then validation reports both conflicting owners

#### Scenario: Duplicate invocation vocabulary is rejected
Given two capability declarations claim the same kind and invocation name
When registry integrity is validated
Then validation reports an ambiguous invocation binding

#### Scenario: Dangling group member is rejected
Given a declared capability group references an unknown capability
When registry integrity is validated
Then validation reports the dangling group member

### Requirement: Slice one remains authority-neutral

Declaration inventory construction must not alter existing tool filtering or execution behavior.

#### Scenario: Callable inventory remains unchanged
Given an existing operator profile and registered tool set
When the declaration inventory is built and validated
Then the legacy callable tool names are unchanged
And tool dispatch continues through the existing EventBus authority path
