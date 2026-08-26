# runtime-contributions/content-packs - Baseline

### Requirement: Shipped content is independently versioned from kernel authority

Skills, prompts, personas, tones, workflows, and catalog data must be distributed and discoverable as versioned content-pack artifacts rather than embedded kernel content. Installation makes content resident but does not grant callability, trust, host effects, or prompt admission. A compatible pack may be upgraded or replaced without rebuilding the kernel artifact.

#### Scenario: Content pack is installed
Given a valid shipped or operator-owned content pack is installed
When contribution discovery runs
Then its content identity, version, digest, provenance, and requested capabilities are inventoried
And no executable asset or prompt body is admitted solely because the pack is resident

#### Scenario: Kernel builds without shipped content
Given shipped content packs are absent from the build inputs
When the constitutional kernel and maintenance artifact are built and started
Then no shipped skill, prompt, persona, workflow, or catalog body is embedded or required
And content inventory reports the packs absent without changing kernel authority

#### Scenario: Content pack is upgraded independently
Given a compatible newer content-pack artifact is installed
When the next content generation is validated and admitted
Then the pack version and digest change without rebuilding the kernel executable
And existing active sessions retain or migrate content generation only under declared policy

### Requirement: Content cannot persist its own trust grants

Content metadata may request paths, effects, tools, or executable assets but must not mutate persistent trust, permission, or workspace policy directly.

#### Scenario: Skill requests a trusted external path
Given a skill declares an external path requirement
When the pack is evaluated
Then the requirement enters admission as a request with provenance
And persistent trusted-path settings remain unchanged until an authorized operator decision is recorded

### Requirement: Missing optional content degrades locally

Absence, corruption, or incompatibility of an optional content pack must not block constitutional-kernel or maintenance startup.

#### Scenario: Shipped catalog pack is missing
Given the maintenance executable starts without the shipped catalog data pack
When diagnostics inspect content inventory
Then the catalog contribution is unavailable with an actionable reason
And the documented maintenance diagnostic and non-destructive denial/quarantine commands remain usable
