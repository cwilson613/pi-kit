# Runtime contribution lifecycle - Delta Spec

## ADDED Requirements

### Requirement: Native extensions pass host-backed conformance

Every first-party native extension must pass one reusable campaign through the
production host discovery, admission, handshake, readiness, and shutdown path.

#### Scenario: Compatible extension is admitted
Given a trusted extension snapshot implements the supported native protocol
When the production host discovers and starts it
Then the host admits its declared capabilities and publishes one generation
And the extension reports its SDK identity and readiness through the shared contract

#### Scenario: Incompatible extension is refused
Given an extension omits or violates a required protocol or readiness contract
When the production host evaluates the candidate
Then the host refuses it before capability publication
And the candidate process tree is settled within the cleanup deadline

### Requirement: Real extension capabilities traverse the host

Conformance must include a real domain invocation through the host adapter and
the admitted extension process.

#### Scenario: Codescan restores search capability
Given the host has admitted the real codescan extension for a temporary workspace
When a caller indexes and searches through the host-owned codescan tools
Then the result contains the expected workspace source hit
And provenance identifies the admitted extension generation

### Requirement: Native extension cancellation is end to end

Cancellation after dispatch must cross the host, transport, and extension worker
without publishing incomplete mutation.

#### Scenario: Active extension request is cancelled
Given an admitted extension has started a mutating request
When the caller cancels that request
Then the host and extension report a cancelled outcome for the same request identity
And incomplete state is not published

### Requirement: Extension failures remain local and owned

Crash, replacement, quarantine, and shutdown must not invalidate unrelated host
or extension capabilities, and every owned process tree must settle.

#### Scenario: One extension exhausts its restart budget
Given two extensions are admitted and one repeatedly crashes
When its restart budget is exhausted
Then only the failing extension becomes quarantined and unavailable
And the other extension and kernel capabilities remain callable

#### Scenario: Host shuts down an extension tree
Given an admitted extension has spawned a descendant process
When the host refuses, replaces, or shuts down that extension
Then the direct child and every owned descendant terminate within the cleanup deadline
And no stale generation accepts a new invocation
