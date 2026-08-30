# Kernel composition artifact profiles - Delta Spec

## ADDED Requirements

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
