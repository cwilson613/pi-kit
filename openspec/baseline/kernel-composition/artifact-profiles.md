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

### Requirement: Extension composition evidence is internally consistent

Runtime inspection, release fixtures, composition locks, and archive validation
must identify host adapters and release-coupled extension artifacts consistently.

#### Scenario: Codescan composition is inspected
Given the full product includes the codescan host adapter and native extension
When a source, linked, or release composition is validated
Then every evidence surface reports the same capability owner vocabulary
And sidecar artifact evidence is not attributed to the host executable digest

### Requirement: Release archives admit exact extension inventories

Archive validation must admit every required declared extension member and reject
members outside the exact release inventory.

#### Scenario: Normal product archive is validated
Given an archive contains the required codescan manifest and executable at canonical paths
When release inventory validation runs
Then validation accepts those extension members
And it still rejects missing, duplicate, misplaced, or unexpected members

### Requirement: Optional-domain evidence executes current contracts

Optional-domain proof data must describe current ownership and invoke executable
absence and degradation checks.

#### Scenario: Codescan optional-domain proof runs
Given codescan is owned by a release-coupled native extension
When the optional-domain isolation gate evaluates codescan
Then it executes current host-absence and extension-degradation tests
And it does not require retired in-process service markers

### Requirement: Composition policy tests run before merge

Pull-request CI must run the maintained composition, packaging, manifest, and
release-policy test suite.

#### Scenario: Composition policy regresses
Given a change makes a release fixture disagree with runtime or packaging behavior
When pull-request CI runs
Then a required pre-merge job fails
And the failure identifies the affected policy test

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
binary graph or unrelated kernel behavior. Each extracted domain must declare
machine-checked kernel absence, additive restoration, and accumulated-product
retention evidence for one canonical service and extension identity.

#### Scenario: Codescan is added to the kernel
Given the kernel-only host reports codescan as typed unavailable
When the admitted codescan sidecar is added
Then host-owned index and search operations become functional
And inventory changes are limited to declared codescan owners and processes

#### Scenario: A future domain joins the accumulated ladder
Given an extracted domain declares its service and extension identities
When its composition policy is validated
Then distinct kernel, additive, and full-product rows contain the declared evidence
And the additive row removes typed absence
And the full-product row retains the extension

### Requirement: Composition budgets include aggregate product cost

Budgets must measure the host, each extension artifact, and their aggregate
installed and runtime cost.

#### Scenario: Extraction moves cost into a sidecar
Given a host dependency or binary payload moves into an extension
When composition budgets are evaluated
Then host and sidecar measurements identify the transfer separately
And aggregate installed cost remains within its declared target-specific bound

### Requirement: Runtime disablement does not rewrite package composition

Disabling a packaged core product component changes runtime eligibility only.
It does not remove package members, weaken signed inventory requirements,
invalidate correct composition locks, or change the artifact profile.

#### Scenario: Full product runs with packaged codescan disabled
Given a valid full-product installation includes locked component `core:codescan`
And effective user policy disables that component
When package and runtime composition are inspected
Then package validation still requires and verifies the codescan members
And runtime inspection reports them as resident but disabled-by-policy

#### Scenario: Disabled policy does not excuse missing package content
Given a full-product package declares required component `core:codescan`
And effective user policy would disable it at runtime
When the package is validated without the codescan executable
Then package validation fails for missing required inventory
And the runtime policy does not weaken that failure

### Requirement: Constitutional artifacts are outside component policy

The resident constitutional kernel, host effects required for safe startup, and
maintenance recovery companion cannot be disabled through product-component
settings.

#### Scenario: Policy attempts to disable the constitutional kernel
Given a user configuration targets a constitutional resident capability
When artifact and component policy are validated
Then the configuration is rejected
And the runtime does not publish a falsely reduced artifact identity

### Requirement: Distribution acceptance isolates component policy

Positive full-product acceptance uses isolated default settings, while negative
acceptance proves a packaged component can be denied without changing installed
inventory or leaving a process behind.

#### Scenario: Installed product acceptance uses default policy
Given a full-product package is installed in an isolated home
When distribution acceptance invokes its core components
Then composition defaults determine eligibility
And inherited operator policy cannot make the positive test nondeterministic

#### Scenario: Installed product honors explicit deny
Given the same installed full-product generation and an explicit `core:codescan` deny
When negative distribution acceptance starts the product
Then codescan remains packaged but no codescan process starts
And typed disabled evidence identifies the policy source

### Requirement: Reduced kernel executes bounded agent turns

The reduced kernel artifact must execute bounded agent turns without importing
product-domain implementations or establishing host-specific route, session, or
invocation authority. Deterministic conformance execution must remain available
without network credentials, while production execution must use the same typed
runtime authorities and terminal outcome contract as full-product bounded work.

#### Scenario: Scripted kernel turn proves the loop boundary
Given the reduced kernel has no provider credentials or optional domain artifacts
When its deterministic conformance turn executes
Then exactly one scripted model request reaches one terminal completion within explicit event and time bounds
And the result identifies scripted conformance provenance without claiming an admitted production route

#### Scenario: Provider-backed kernel turn completes
Given the reduced kernel has an admitted provider route and a bounded task
When the operator invokes its bounded execution entrypoint
Then route selection, request execution, and terminal settlement use shared typed runtime authorities
And the structured result and exit status match the bounded task contract without loading product domains

#### Scenario: Kernel turn exhausts a bound
Given a reduced-kernel turn reaches its admitted time, turn, token, event, or tool bound
When the next governed action would exceed that bound
Then execution stops before that action with a typed exhausted outcome
And provider, invocation, and child-process authority settle before exit

### Requirement: First-party domains have explicit packaging classes

Every first-party runtime domain must appear exactly once in a checked inventory
that records its canonical owner, packaging class, runtime boundary, extraction
disposition, and composition evidence. A domain may be constitutional resident,
host service, signed core component, shipped content, or operator-managed SDK
extension; runtime disablement does not rewrite that packaging class.

#### Scenario: First-party domain inventory is checked
Given the workspace adds or retains a first-party runtime domain
When composition policy validation runs
Then the domain has exactly one packaging class, canonical owner, and extraction disposition
And an unknown, duplicate, or contradictory classification fails the gate

#### Scenario: Managed service remains in the host
Given a managed service has authority, cost, or failure-boundary evidence that does not justify extraction
When its classification is reviewed
Then it remains an explicit host service with typed lifecycle ownership
And the inventory does not falsely present it as a signed core component

#### Scenario: Domain graduates to a core component
Given a first-party domain is classified as a signed `core:*` component
When its promotion is validated
Then portable contracts, signed identity, kernel absence, additive restoration, full-product retention, cleanup, and aggregate budget evidence are present
And SDK extension metadata cannot grant that product-component authority

#### Scenario: Core qualification evidence is incomplete
Given a signed `core:*` qualification record omits, duplicates, or aliases a required executor
When composition policy validation runs
Then promotion fails before package or runtime publication
And evidence from another component or an SDK extension cannot satisfy the missing boundary

#### Scenario: Core qualification evidence is complete
Given a component has portable contracts and a signed release identity
And its qualification record names kernel-absence, additive-restoration, full-product, policy, protocol, cleanup, budget, inventory, and non-promotion executors
When the generic component promotion gate runs
Then every executor passes for the same component and wire identity
And the component becomes eligible for signed-core promotion without a component-specific policy exception
