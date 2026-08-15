# Inference model admission — Delta Spec

## ADDED Requirements

### Requirement: Evidence-backed admission

Every runtime inference offering shall expose a deterministic admission status derived from route state and field-level evidence.

#### Scenario: Curated offering
Given an enabled offering declared by the embedded or manifest inventory
When admission is derived
Then its status is curated
And no model-name heuristic is used

#### Scenario: Newly discovered offering
Given an enabled provider-discovered offering with no stronger evidence
When admission is derived
Then its status is provisional
And it remains available for exact explicit selection
And it remains ungraded for autonomous routing

#### Scenario: Probed offering
Given an enabled offering with successful probed evidence
When admission is derived
Then its status is observed

#### Scenario: Quarantined offering
Given an offering carrying explicit quarantine evidence
When admission is derived
Then its status is quarantined
And it is not reported as available

#### Scenario: Unavailable offering
Given a disabled offering or endpoint
When admission is derived
Then its status is unavailable

### Requirement: Truthful operator projection

Model inventory output shall distinguish admission state from credential availability.

#### Scenario: Model list includes admission
Given a catalog containing provisional and curated routes
When the operator lists models
Then each route reports its admission status
And unknown context or capability metadata is not synthesized from its name
