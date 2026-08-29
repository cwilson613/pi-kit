# provider-routing/parity - Baseline

### Requirement: Exact routes fail closed against current inventory

An exact model route must resolve to one enabled, admitted offering whose
endpoint interface, modalities, and required capabilities satisfy the request.
Unknown provider or model identities must not silently route to a default
provider.

#### Scenario: Unknown exact model is selected
Given the active inference inventory contains no offering for an exact model
When the operator requests that exact model
Then route selection is rejected before bridge replacement
And the previously active route remains unchanged

#### Scenario: Exact tool route lacks tool capability
Given an exact offering exists but its current evidence does not support tools
When a request containing tool definitions is admitted
Then provider dispatch is rejected before network activity
And the rejection identifies the missing tool capability

### Requirement: Supported manifest HTTP endpoints are executable

An admitted manifest endpoint using a host-supported HTTP adapter may construct
an executable provider bridge from its validated transport, native model ID,
secret references, capability evidence, and contribution generation. Unknown
adapters and unadmitted endpoints remain non-executable. Remote endpoints must
use HTTPS. Loopback endpoints may use HTTP. An endpoint may resolve only the
dedicated secret name derived from its inventory source and collision-free
endpoint ID encoding.

Sessionless manifest route leases add optional endpoint ID, adapter ID, and
inventory generation fields. Existing sessionless records remain readable when
the fields are absent. Session-backed routes retain the frozen
`route.lease_recorded` v1 payload and append a linked
`route.endpoint_provenance_recorded` v1 fact. Manifest routes use the host
adapter contribution generation instead of inventing a provider contribution.
Session-idle compaction appends equivalent endpoint provenance through
`compaction.endpoint_provenance_recorded` v1.

#### Scenario: Admitted OpenAI-compatible endpoint is selected
Given a manifest declares an enabled HTTP endpoint using the supported OpenAI-compatible chat adapter
And its offering has sufficient evidence and a resolvable secret reference
And a remote endpoint uses HTTPS or a loopback endpoint uses HTTP
When route resolution selects that offering
Then Omegon constructs an executable bridge using the declared base URL and native model ID
And the route lease retains the endpoint and contribution generation identity

#### Scenario: Manifest endpoint uses an unsupported adapter
Given a manifest endpoint names an adapter the host does not implement
When route resolution evaluates its offering
Then the offering remains non-executable
And no generic protocol guess or network dispatch occurs

### Requirement: Retry policy honors bounded server-directed backoff

When same-route retry is otherwise permitted, a valid `Retry-After` or
`Retry-After-Ms` value from the failed response must set the next delay subject
to Omegon's existing retry and exhaustion ceilings. Invalid or absent values use
the existing jittered exponential policy.

#### Scenario: Provider supplies retry delay in seconds
Given a retryable provider failure includes `Retry-After: 17`
When central retry policy schedules the next same-route attempt
Then the scheduled delay is at least 17 seconds
And durable failed-attempt evidence precedes the retry

#### Scenario: Provider supplies malformed retry delay
Given a retryable provider failure includes an invalid `Retry-After` value
When central retry policy schedules the next same-route attempt
Then it uses the bounded jittered exponential delay
And it does not extend the existing retry exhaustion envelope

### Requirement: Provider preference participates in candidate ordering

The session's configured provider order must deterministically rank otherwise
eligible candidates without admitting a disabled, avoided, under-grade, or
credentialless candidate.

#### Scenario: Equal candidates follow provider order
Given OpenAI and Anthropic candidates are equally eligible for a requested grade
And session policy orders Anthropic before OpenAI
When routing scores the candidates
Then the Anthropic candidate sorts before the OpenAI candidate
And both candidates retain their capability-derived scores and eligibility

### Requirement: Model evidence gates tools and reasoning

Provider-level schema dialect remains authoritative for normalization, while the
selected offering's current evidence determines whether tools and explicit
reasoning controls may be sent.

#### Scenario: Explicit reasoning is unsupported
Given the selected offering explicitly lacks reasoning capability
When route preparation receives an explicit reasoning level
Then the unsupported reasoning control is not dispatched
And route preparation reports the capability mismatch

#### Scenario: Tool support and dialect agree
Given the selected offering has sufficient tool capability evidence
And its provider contribution declares a supported schema dialect
When a tool-bearing request is prepared
Then the tool schema is normalized through that declared dialect
And the provider receives the normalized tools

### Requirement: Environment credential kind is recorded accurately

Credential probing must classify the selected environment variable according to
the provider's declared authentication semantics so OAuth tokens are not recorded
as API keys.

#### Scenario: OAuth environment token is selected
Given a provider accepts OAuth and its OAuth environment variable contains the selected credential
When the route credential ledger probes that provider
Then the credential state records an environment source with OAuth kind
And the resulting route metadata does not claim API-key authentication

### Requirement: Selected and native model identities remain distinct

When an offering's selected identity differs from its native upstream model ID,
the selected identity must remain operator-visible and durable while transport
uses only the admitted native model ID.

#### Scenario: Aliased offering is dispatched
Given an admitted offering named `stable-chat` maps to native model `model-v3`
When a request is dispatched through that offering
Then the route lease records `stable-chat` as the selected identity
And provider transport sends `model-v3` as the native model identity
