# Skills Progressive Disclosure — Delta Spec

## ADDED Requirements

### Requirement: Skill admission is deterministic and evidence-based

The skill inventory must derive disclosure decisions from declared activation metadata and current workspace or operator evidence without inferring admission from the skill name.

#### Scenario: Always-active skill is admitted
Given an installed skill declares always-active activation
When disclosure is projected for a session
Then the skill is admitted into prompt context
And its installed body is available to prompt construction

#### Scenario: Workspace signal controls conditional admission
Given an installed skill declares a literal or shallow-glob workspace signal
When disclosure is projected for a matching workspace
Then the skill is admitted
And matching checks use path existence without reading file contents

#### Scenario: Unmatched skill remains resident only
Given an installed skill has no matching workspace or operator evidence
When disclosure is projected
Then the skill remains installed but is not admitted into prompt context

#### Scenario: Missing activation does not imply admission
Given an installed skill has absent or unknown activation metadata
When disclosure is projected
Then the skill is not admitted solely from its name or description

### Requirement: Skill descriptions are usable retrieval keys

Skill diagnostics must identify descriptions that cannot reliably support retrieval and activation decisions.

#### Scenario: Unusable descriptions are reported
Given a skill description is missing, shorter than 24 characters, or a known placeholder
When retrieval-key lint runs
Then a diagnostic finding identifies the skill description as unusable

#### Scenario: Doctor summarizes external findings
Given one or more installed external skills fail retrieval-key lint
When the operator runs the skill doctor
Then the operator-facing summary includes the retrieval-key findings

#### Scenario: Bundled skills satisfy retrieval lint
Given the bundled `skills/*/SKILL.md` inventory
When retrieval-key lint runs
Then every bundled skill passes
