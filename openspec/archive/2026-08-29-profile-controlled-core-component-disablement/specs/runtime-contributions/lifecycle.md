# Runtime contribution lifecycle - Delta Spec

## ADDED Requirements

### Requirement: Component policy is enforced before execution

An effective deny excludes a core product component before process creation,
handshake, readiness, mutable engine access, or contribution publication.
Unrelated admitted contributions remain unchanged.

#### Scenario: Codescan is disabled before boot
Given packaged component `core:codescan` is denied by effective policy
When the runtime composes its contribution generation
Then no codescan process, handshake, readiness probe, index, or database mutation occurs
And unrelated host and component contributions remain eligible

#### Scenario: Disabled component has a required dependent
Given a non-disableable contribution requires a component denied by effective policy
When the contribution graph is validated
Then runtime publication is rejected as contradictory configuration
And the denied component is not started to repair the contradiction

#### Scenario: Disabled component has only optional dependents
Given optional contributions depend on a component denied by effective policy
When the contribution graph is validated
Then the component and those optional dependents are omitted deterministically
And diagnostics identify the dependency-based omissions

### Requirement: Disabled is a typed runtime state

A packaged component denied by policy is reported as `disabled-by-policy`, not
absent, incompatible, failed, or quarantined. Its component-backed tools are not
model-callable, while direct invocation returns typed `service:disabled`
evidence with policy provenance.

#### Scenario: Model tool inventory excludes disabled codescan
Given packaged `core:codescan` is disabled by effective policy
When model-callable tools are projected
Then codescan-backed tools are excluded
And unrelated callable tools are unchanged

#### Scenario: Direct invocation reaches a disabled adapter
Given packaged `core:codescan` is disabled by the selected profile
When a CLI, ACP, or direct tool caller invokes codescan
Then the host returns typed `service:disabled`
And the response identifies `core:codescan` and the determining policy source

### Requirement: Component policy changes are generation-bound

Profile edits do not silently mutate components captured by an active session.
The new policy applies on the next runtime boot unless a separately specified
quiescent migration protocol is used.

#### Scenario: Active session remains stable after profile edit
Given an active session captured a healthy codescan component
When the operator disables `core:codescan` in the selected profile
Then the active generation remains unchanged
And the command reports that the deny takes effect after restart

#### Scenario: Re-enabled packaged component starts after restart
Given packaged `core:codescan` was disabled without being uninstalled
And effective policy is changed to allow it
When a new runtime boot composes the generation
Then the packaged component passes normal admission and readiness
And codescan search becomes available without reinstallation
