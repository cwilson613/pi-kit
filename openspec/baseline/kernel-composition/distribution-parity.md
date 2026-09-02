# kernel-composition/distribution-parity - Baseline

### Requirement: Distributions declare host and product composition

Every enabled distribution must declare a target-specific compiled host artifact
profile, installation composition class, exact signed `core:*` product-component
inventory, and SDK-extension posture. Candidate package surfaces may retain the
same checked policy before publication, but they are not supported public channels
until stable-release work enables and validates their live lanes. Retired
distribution scaffolding must be explicitly unsupported.

#### Scenario: Distribution policy is evaluated
Given a release archive or direct installer is enabled, or a Homebrew, Nix, or OCI output is retained as a pre-publication candidate
When distribution policy validation runs
Then the output has one explicit host profile, composition class, and core-component inventory
And policy coverage does not claim that a candidate channel was published or supported

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

### Requirement: Enabled activation paths authenticate exact composition

Every supported installation or activation path must authenticate the exact host,
required component, content, target, and composition identity through signed
release evidence or a documented channel-native trust boundary. Missing tooling,
missing evidence, identity mismatch, or unsupported composition must fail before
activation and must not downgrade to checksum-only success.

For this pre-publication milestone, release archives, direct installation, and
version switching are enabled acceptance paths. Homebrew, Nix, and OCI retain
checked composition and trust policy, but live channel acceptance and publication
are deferred to stable-release work.

#### Scenario: Direct installer lacks signature verification
Given a direct-install candidate has a valid checksum but its signature evidence cannot be verified
When the installer evaluates the candidate
Then installation fails before any active generation changes
And diagnostics identify unavailable or invalid authenticity evidence

#### Scenario: Fresh install authenticates before extraction
Given a fresh machine has an approved external bootstrap verifier
And the archive, package manifest, and signature bundle identify one release, target, and composition
When the direct installer admits the candidate
Then external verification succeeds before executable extraction or generation staging
And the installed maintenance companion revalidates the exact extracted composition before activation

#### Scenario: Direct-install verification fails after staging starts
Given an active generation exists and a direct-install candidate fails authenticity or composition validation
When installation settles
Then the active generation remains unchanged and callable
And the candidate download and generation staging directories are removed

#### Scenario: Version switch targets another generation
Given an installed generation and a requested switch target both have composition metadata
When switching validates the target
Then it verifies exact signed host and required-component evidence before activation
And failure preserves the previously callable generation

#### Scenario: Switch candidate attempts self-verification
Given a verified active generation and an untrusted switch candidate both contain a maintenance executable
When switch verification begins
Then only the active generation's maintenance authority can verify the candidate
And no executable or metadata from the candidate can grant its own activation authority

#### Scenario: Switch publication is interrupted
Given a verified target generation is staged and the prior generation is active
When publication fails before the atomic selector changes
Then host, maintenance companion, required components, content, receipt, and locks still resolve to the prior generation
And recovery identifies the prior active generation and removes incomplete staging

#### Scenario: Nix policy declares a host-only derivation
Given the candidate Nix output intentionally omits release-coupled core components
When distribution policy and derivation evidence are checked
Then exact source or artifact pins and the channel trust boundary are verified
And the output remains explicitly host-only with typed component unavailability

#### Scenario: OCI release evidence is incomplete
Given an OCI candidate lacks its digest-bound signature, SBOM, provenance, or exact composition identity
When deterministic OCI policy acceptance runs
Then the candidate is rejected as unsupported release evidence
And no full-product or host-only readiness claim is published for it

#### Scenario: OCI evidence binds one image digest
Given an OCI candidate supplies a signature, SBOM, provenance, and composition statement
When the CI distribution verifier evaluates the candidate
Then every evidence document identifies the same immutable image digest and composition class
And verification records policy evidence without claiming that production publication occurred
