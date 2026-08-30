# kernel-composition/distribution-parity - Baseline

### Requirement: Distributions declare host and product composition

Every supported distribution must declare a target-specific compiled host
artifact profile, installation composition class, exact signed `core:*` product-
component inventory, and SDK-extension posture. Retired distribution scaffolding
must be explicitly unsupported.

#### Scenario: Distribution policy is evaluated
Given a release archive, direct installer, Homebrew, Nix, or OCI output is supported
When distribution policy validation runs
Then the output has one explicit host profile, composition class, and core-component inventory
And a host-only output does not claim full-product capability parity

#### Scenario: Retired npm scaffolding is evaluated
Given npm packaging files remain in the repository but npm is not a supported channel
When distribution policy validation runs
Then npm is declared unsupported rather than assigned a current composition class
And release publication does not consume the stale package metadata

### Requirement: Full-product packages prove installed extension behavior

A full-product distribution must pass functional acceptance after installation,
not only archive-member validation. Required core-component inventory describes
package membership and integrity; it does not require unconditional runtime
activation when component policy denies a packaged extension.

#### Scenario: Full-product archive is installed
Given a normal release archive is extracted into an isolated prefix with a clean default profile
When the installed host invokes the codescan acceptance operation
Then it discovers and uses packaged component `core:codescan`
And the complete extension process tree settles when the host exits

### Requirement: Product-component artifacts have independent provenance

Release-coupled product components must have artifact evidence bound to their
component identity, wire manifest identity, own bytes, protocol, target,
fallback, and signing identity. SDK manifests cannot grant product-component
authority.

#### Scenario: Sidecar provenance is verified
Given a package declares release-coupled component `core:codescan`
When package and runtime admission evidence are validated
Then both identify the same manifest and executable digests
And neither attributes the sidecar payload to the host executable digest

### Requirement: Host and required product components publish atomically

Installation, update, and rollback must expose one internally consistent host and
required-component generation while preserving operator-managed SDK extensions.

#### Scenario: Extension staging fails during update
Given the active generation has a callable host and `core:codescan` component
When the next product-component generation fails staging or verification
Then activation does not publish any part of the candidate generation
And the previous host and component remain callable
