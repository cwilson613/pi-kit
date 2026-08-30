# Settings component policy - Delta Spec

## ADDED Requirements

### Requirement: Core component activation is explicitly configurable

The selected profile may enable or disable a release-declared, disableable core
product component by stable `core:*` identity. An omitted setting preserves the
signed product composition default.

#### Scenario: Profile disables packaged codescan
Given the active product composition declares disableable component `core:codescan`
And the selected profile sets `core:codescan` enabled to false
When effective component policy is resolved
Then `core:codescan` is denied for the next runtime boot
And its packaged files remain part of the product composition

#### Scenario: Profile explicitly enables a component
Given the active product composition declares available component `core:codescan`
And no higher-authority policy denies it
When the selected profile sets `core:codescan` enabled to true
Then the component is eligible for normal compatibility, readiness, and admission checks

### Requirement: User-local denies are monotonic

A user-local component policy may deny exact disableable component IDs or the
`core:*` selector across project-selected profiles. Profile enables and child
configuration cannot override that deny.

#### Scenario: Project profile cannot override user deny
Given user-local policy denies `core:codescan`
And the selected project profile enables `core:codescan`
When effective component policy is resolved
Then `core:codescan` remains denied
And diagnostics attribute the effective decision to the user-local policy source

#### Scenario: Wildcard deny covers a future declared component
Given user-local policy denies `core:*`
And the active composition declares multiple disableable core components
When effective component policy is resolved
Then every declared disableable core component is denied
And resident constitutional capabilities remain unaffected

### Requirement: Component policy validation is strict and source-aware

Runtime parsing and published schema validation reject the same unknown fields,
invalid types, malformed selectors, unknown exact component IDs, and attempts to
disable non-disableable capabilities. Errors identify the configuration source
and issue path before component execution begins.

#### Scenario: Misspelled component deny is rejected
Given a profile contains exact selector `core:codesan`
When the profile is validated against the active composition catalog
Then validation fails before component discovery
And the error identifies the selector and profile source path

#### Scenario: Unknown policy key is rejected
Given a component entry uses `enabeld` instead of `enabled`
When runtime and schema validation evaluate the profile
Then both reject the unknown key
And neither silently drops the intended deny

#### Scenario: Constitutional capability cannot be disabled
Given configuration targets a resident constitutional kernel capability
When component policy is validated
Then validation fails because the target is not a disableable product component

### Requirement: Legacy codescan denies migrate without broadening authority

Existing generic extension policy that denies `omegon-codescan` retains its
runtime effect and is migrated to component identity without changing unrelated
SDK-extension policy.

#### Scenario: Legacy codescan deny is loaded
Given an existing profile lists `omegon-codescan` in generic extension denies
When the profile is loaded and normalized
Then effective component policy denies `core:codescan`
And diagnostics identify the deprecated source field

#### Scenario: Migrated profile is saved
Given a loaded profile contains a legacy codescan deny and unrelated extension denies
When the profile is saved through the canonical profile target
Then it writes an explicit `core:codescan` component deny
And it preserves unrelated extension denies

### Requirement: Component settings expose effective provenance

Settings surfaces report packaged identity, composition default, each applicable
policy source, final eligibility, and restart requirements for every core product
component.

#### Scenario: Operator inspects a denied component
Given `core:codescan` is packaged and denied by the selected profile
When the operator requests effective component settings
Then the response distinguishes packaged from active state
And it identifies the selected profile source and restart-bound effect
