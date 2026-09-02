# milestone/readiness-gate - Baseline

### Requirement: Milestone completion gates pull-request readiness

The operational kernel and core milestone must not be declared pull-request ready
until every planned task and delta scenario is complete, documentation matches
the implemented behavior, applicable repository landing gates pass, and OpenSpec
validation proves the deltas can be archived cleanly.

#### Scenario: Milestone has incomplete work
Given any implementation task, scenario verification, documentation update, or required landing gate is incomplete
When pull-request readiness is evaluated
Then the milestone remains not ready
And the remaining task or unavailable evidence is reported explicitly

#### Scenario: Milestone reaches readiness
Given every milestone task and scenario has implementation evidence
When named OpenSpec, test, lint, composition, applicable distribution-policy, and documentation gates pass
Then archive-check confirms the deltas can merge into the baseline
And the branch may be declared ready for pull-request review

### Requirement: Promotion evidence is scenario-indexed and boundary-specific

The repository must maintain a checked acceptance corpus with stable scenario
identities, explicit authority invariants, observable oracles, evidence status,
executor references, and promotion-profile membership. Scripted conformance,
provider-backed kernel acceptance, signed core-component promotion, SDK-addon
promotion, and milestone readiness must remain separate gates.

#### Scenario: Planned evidence is presented to a promotion gate
<!-- id: milestone/corpus-planned-evidence -->
Given a promotion profile contains a scenario whose evidence remains planned
When the profile gate is evaluated
Then the profile remains incomplete and identifies the planned scenario
And evidence from another profile cannot substitute for the missing boundary

#### Scenario: A component claims the wrong promotion class
<!-- id: milestone/corpus-promotion-class -->
Given an SDK addon or scripted provider claims signed core or production route authority
When corpus and composition validation run
Then the claim is rejected by the applicable promotion profile
And package metadata or test provenance is not reinterpreted as stronger authority
