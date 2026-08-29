# Artifact profiles - Delta Spec

## ADDED Requirements

### Requirement: Extension growth uses an executable composition ladder

The repository must test physically distinct kernel-only, additive-extension,
and accumulated full-product compositions.

#### Scenario: Composition ladder runs
Given kernel-only, kernel-plus-codescan, and full-product rows are declared
When the composition matrix executes
Then each row builds or installs its declared artifact set
And each row starts and performs a representative functional operation

### Requirement: Kernel boundaries are positive and executable

The kernel-only row must enforce an explicit dependency and resident-capability
policy and remain useful without optional domains.

#### Scenario: Kernel-only artifact starts
Given every optional extension artifact is absent
When the kernel-only artifact starts in an isolated state root
Then its dependency and resident inventories satisfy the positive kernel policy
And one core operation completes without optional-domain fallback discovery

### Requirement: Additive extensions restore only declared capability

Adding an extension must restore its declared behavior without changing the host
binary graph or unrelated kernel behavior.

#### Scenario: Codescan is added to the kernel
Given the kernel-only host reports codescan as typed unavailable
When the admitted codescan sidecar is added
Then host-owned index and search operations become functional
And inventory changes are limited to declared codescan owners and processes

### Requirement: Composition budgets include aggregate product cost

Budgets must measure the host, each extension artifact, and their aggregate
installed and runtime cost.

#### Scenario: Extraction moves cost into a sidecar
Given a host dependency or binary payload moves into an extension
When composition budgets are evaluated
Then host and sidecar measurements identify the transfer separately
And aggregate installed cost remains within its declared target-specific bound
