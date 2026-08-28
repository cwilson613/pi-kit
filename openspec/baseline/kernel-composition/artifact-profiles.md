# kernel-composition/artifact-profiles - Baseline

### Requirement: External domain composition

The Omegon integration package must not link independently stateful optional
domain engines when the domain can run behind a supervised native extension
contract. Cargo features remain additive for code that belongs in the host
artifact; release composition selects companion extension artifacts separately.

#### Scenario: Existing full artifact
Given the default Omegon product is packaged
When its artifact inventory is assembled
Then the host and release-coupled codescan extension are both installed
And the host binary does not link the codescan engine

#### Scenario: Shrinking composition base
Given any supported Omegon feature composition
When Cargo resolves the package dependency graph
Then the codescan engine is absent from that graph

### Requirement: Contract and engine separation

Portable versioned request, response, status, cancellation, and error types remain
independent from the domain engine and its implementation dependencies.

#### Scenario: Domain engine omitted
Given the codescan extension is absent, incompatible, or unavailable
When the integration runtime declares or invokes that domain capability
Then it reports typed unavailability without linking or directly invoking the engine

### Requirement: Bundled native codescan extension

The release-coupled codescan extension exclusively owns workspace indexing,
SQLite, freshness checks, and BM25 construction behind a versioned native RPC
contract. Tool search, explicit indexing, and code-context search share one
serial worker and propagate request cancellation.

#### Scenario: Extension serves host adapters
Given a compatible codescan extension is admitted at boot
When a host tool or code-context request invokes codescan
Then the request crosses the captured extension RPC handle
And no host adapter opens the database or invokes the engine directly

#### Scenario: Active request is cancelled
Given the extension is indexing for an admitted request
When caller or host-generation cancellation occurs
Then the host sends cancellation for that request identity
And the extension rolls back incomplete publication before returning cancelled

### Requirement: Dependency subtraction ratchet

The repository verifies that the codescan engine does not appear in any Omegon
package dependency graph and that removed presentation dependencies do not appear
in the shrinking no-default graph.

#### Scenario: Forbidden dependency reintroduced
Given the codescan engine enters any Omegon feature graph or a forbidden
presentation dependency enters the no-default graph
When the dependency-boundary check runs
Then the check fails and identifies the reintroduced package
