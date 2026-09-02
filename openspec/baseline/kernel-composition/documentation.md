# kernel-composition/documentation - Baseline

### Requirement: Every implementation lane declares documentation impact before mutation

Each implementation lane must identify its durable architecture/developer documentation impact and its public operator documentation impact before implementation begins. A lane with no public-facing change must record that determination and its evidence rather than silently omitting documentation work.

#### Scenario: Lane changes an internal contract only
Given a lane changes a serialized kernel contract without changing an operator command or workflow
When its implementation plan is refined
Then the lane identifies the owning durable document and compatibility notes to update
And records why no public site change is required

#### Scenario: Lane changes operator-visible behavior
Given a lane changes commands, configuration, output, recovery workflow, packaging, permissions, or availability
When its implementation plan is refined
Then the lane names the affected `site/src/pages/docs/` pages and shared `site/snippets/` sources before code mutation begins

### Requirement: Documentation ships in the same lane as behavior

Required durable documentation, public site pages, shared command snippets, examples, migration guidance, and operator warnings must be updated and validated before the lane's exit gate passes. Documentation must describe the behavior actually implemented by that lane and must not be deferred to the final release group.

#### Scenario: Command contract lands
Given a lane adds or changes an operator command
When the lane is proposed for completion
Then durable command/architecture documentation and applicable public site/snippet content are present in the same change series
And examples use the implemented command, arguments, output shape, and safety behavior

#### Scenario: Internal implementation diverges from drafted design
Given implementation evidence requires changing a documented design assumption
When the lane reconciles its exit gate
Then the source design, OpenSpec requirements/tasks, and affected public documentation are corrected before completion
And stale aspirational behavior is not left as current guidance

### Requirement: Documentation is validated as an artifact

Each lane must run the narrowest available validation for changed durable docs, public site pages, links, and shared snippets. A release-facing lane must additionally prove generated or rendered public documentation consumes the canonical snippets and matches packaged behavior.

#### Scenario: Public documentation changes
Given a lane modifies public site documentation or shared snippets
When validation runs
Then relevant site tests and build checks pass
And broken links, stale command examples, or duplicate noncanonical snippets fail the lane gate

### Requirement: Cross-surface terminology remains canonical

Kernel, maintenance, contribution, capability, generation, lease, session, route, and recovery terminology must remain consistent across source design, OpenSpec, developer docs, public site pages, CLI help, and semantic operator surfaces.

#### Scenario: New runtime state is introduced
Given a lane introduces a new contribution or invocation state
When documentation and operator projections are reviewed
Then every surface uses the canonical state name and defined meaning
And no renderer or public page invents a conflicting synonym with different semantics

### Requirement: Product documentation distinguishes implemented composition boundaries

Architecture, operator, installation, security, and extension documentation must
distinguish the full integration host, reduced kernel artifact, host services,
signed core components, shipped content, and SDK extensions. Claims of capability
or trust parity must be backed by executable evidence for the named artifact and
distribution channel; planned behavior must be labeled as planned.

#### Scenario: Kernel and core architecture is documented
Given the reduced kernel and full product have different executable capabilities
When architecture and operator documentation are validated
Then each artifact's supported operations and component boundaries match executable evidence
And a conformance probe is not presented as production provider-backed execution

#### Scenario: Distribution channels have different composition
Given native archives are full-product while a supported channel is host-only
When installation and security guidance is rendered
Then each channel states its exact component inventory, trust boundary, and typed unavailable behavior
And no host-only channel inherits a full-product readiness claim
