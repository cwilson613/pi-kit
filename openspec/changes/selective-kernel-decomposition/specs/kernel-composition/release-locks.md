# Kernel composition: release locks and budgets - Delta Spec

## ADDED Requirements

### Requirement: Release artifacts declare their required composition

Every supported release package must carry a signed, verifiable package composition lock listing required companion artifacts. Each executable artifact must also carry its resident required/optional module and contribution identity, artifact digest, protocol range, target support, and fallback behavior. Signature identity and verification result are part of release evidence.

#### Scenario: Required module is missing from a package
Given a release package's lock requires the companion maintenance executable
When package verification cannot resolve its signed artifact identity, digest, and protocol range
Then the package fails release verification
And the interactive executable's resident-module lock does not falsely claim that the companion maintenance executable or its workflows are resident

#### Scenario: Composition lock signature is invalid
Given a package or executable composition lock has an unknown or invalid signature
When release or startup verification evaluates the lock
Then verification fails closed for required composition
And diagnostics identify the failed signing identity without executing optional contributions

### Requirement: Product profiles have gated composition matrices

Maintenance, interactive, headless, daemon, and full profiles must be built and exercised through every supported source, linked-development, and release packaging path applicable to that profile.

#### Scenario: Optional domain is absent from headless
Given the headless profile marks lifecycle and TUI optional or absent
When the headless composition matrix runs
Then startup and bounded execution pass without those domains
And inventory reports their composition state honestly

### Requirement: Composition budgets prevent kernel regrowth

CI must record and enforce approved limits or deltas for dependency count, binary size, startup task count, model-schema tokens, resident capabilities, and default callable capabilities for maintenance and normal product profiles.

#### Scenario: Optional extraction increases maintenance dependencies
Given a change adds an optional domain dependency to the maintenance artifact
When composition budgets are evaluated
Then the unapproved regression fails the gate
And diagnostics attribute the dependency and size delta to its contribution owner
