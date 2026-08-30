# Artifact profiles - Delta Spec

## ADDED Requirements

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
