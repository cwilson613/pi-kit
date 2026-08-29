# Distribution parity - Delta Spec

## ADDED Requirements

### Requirement: Distributions declare extension composition

Every supported distribution must declare a target-specific `full-product` or
`host-only` composition and its exact required extension inventory.

#### Scenario: Distribution policy is evaluated
Given a release archive, direct installer, Homebrew, npm, Nix, or OCI output is supported
When distribution policy validation runs
Then the output has one explicit composition class and extension inventory
And a host-only output does not claim full-product capability parity

### Requirement: Full-product packages prove installed extension behavior

A full-product distribution must pass functional acceptance after installation,
not only archive-member validation.

#### Scenario: Full-product archive is installed
Given a normal release archive is extracted into an isolated prefix
When the installed host invokes the codescan acceptance operation
Then it discovers and uses the packaged codescan extension
And the complete extension process tree settles when the host exits

### Requirement: Extension artifacts have independent provenance

Release-coupled extensions must have artifact evidence bound to their own bytes,
protocol, target, fallback, and signing identity.

#### Scenario: Sidecar provenance is verified
Given a package declares a release-coupled extension
When package and runtime admission evidence are validated
Then both identify the same manifest and executable digests
And neither attributes the sidecar payload to the host executable digest

### Requirement: Host and required extensions activate atomically

Installation, update, and rollback must expose one internally consistent host and
required-extension generation.

#### Scenario: Extension staging fails during update
Given the active generation has a callable host and codescan extension
When the next extension generation fails staging or verification
Then activation does not publish any part of the candidate generation
And the previous host and extension remain callable
