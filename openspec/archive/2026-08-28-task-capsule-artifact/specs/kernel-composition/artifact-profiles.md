# Artifact profiles - Delta Spec

## ADDED Requirements

### Requirement: Bounded task capsule artifact

The repository must define `task-capsule-v0` as an explicit compiled artifact
profile whose canonical entrypoint is `omegon run`. The capsule must preserve the
bounded task execution contract while reporting its artifact identity from
compiled features rather than a caller-selected runtime label.

#### Scenario: Capsule is built
Given the task capsule build command is invoked
When Cargo resolves and compiles the Omegon package
Then it disables default features and enables only the capsule marker
And the resulting binary reports `task-capsule-v0` as its compiled artifact profile
And adding TUI, self-update, or local-embedding features fails compilation

#### Scenario: Capsule executes bounded work
Given a capsule has an admitted provider route and a bounded task
When the operator invokes `omegon run`
Then the existing structured result and exit-code contract remains available
And execution does not require a TUI, codescan extension, or self-update capability

#### Scenario: Product profile is requested from a capsule
Given Omegon was compiled as `task-capsule-v0`
When composition inspection requests an interactive, daemon, or full profile
Then inspection rejects the incompatible profile
And it does not report product-only surfaces as resident capsule capabilities
And rejection occurs before workspace or runtime state is created

### Requirement: Capsule dependency subtraction ratchet

The capsule dependency graph must exclude every package explicitly subtracted
from the artifact. V0 excludes presentation dependencies, the codescan engine,
and self-update signature verification while retaining all other current runtime
domains until separate absence contracts are implemented.

#### Scenario: Excluded dependency is reintroduced
Given a TUI, codescan-engine, Sigstore, or X.509 parser package enters the capsule graph
When the capsule dependency check runs
Then the check fails and identifies the reintroduced package

#### Scenario: Self-update is requested from the capsule
Given Omegon was compiled without self-update support
When an update installation path requests signature verification and replacement
Then the operation returns explicit compiled-capability unavailability
And no download, archive, receipt, or process mutation begins
