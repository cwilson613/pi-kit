# OpenCode routing parity design

## Reference and authority

The behavioral reference is `anomalyco/opencode` at
`c77100a40c16a1c7c39115023ccd6f284b476c77`. OpenCode is implementation
evidence, not architectural authority. Omegon's contribution registry, inference
inventory, credential manager, route service, and durable route lease remain the
authoritative boundaries.

## Decisions

### Admission precedes construction

Exact selection will resolve an inventory offering and run compatibility checks
before replacing a bridge. Bridge construction consumes that admitted result;
it does not independently infer provider, capability, or endpoint policy.

Bare convenience model names may still be canonicalized by an explicit catalog
match. An unrecognized identity is an error rather than an Anthropic fallback.

### Manifest execution is adapter-bounded

The first executable manifest transport is HTTP with the existing
OpenAI-compatible chat adapter. The host will not dynamically load code or infer
protocol compatibility from URL shape. Endpoint base URL, secret reference,
native model ID, capability evidence, and inventory generation are captured
before bridge construction.

Remote endpoints require HTTPS. Plain HTTP is allowed only for loopback hosts.
Each endpoint can resolve only its dedicated
`OMEGON_<SOURCE>_ENDPOINT_<HEX_ENDPOINT_ID>_TOKEN` secret. The source and
collision-free endpoint encoding bind the secret to the inventory owner. This
binding prevents a project manifest from claiming an unrelated user or provider
credential.

### Capability checks are model-level

Provider contributions continue to own schema dialect and authentication class.
Offerings own current model-level evidence. A request must satisfy both: provider
tool support cannot override an offering that lacks tool evidence, and offering
tool evidence cannot invent a provider schema dialect.

### Retry timing remains central

Provider adapters may capture bounded response metadata, but only the route
service schedules retries. Server-directed delay is advisory within existing
attempt, duration, cancellation, and durable-evidence rules. It does not authorize
retry or cross-provider failover by itself.

### Preference is a tie-break, not admission

Configured provider order affects deterministic candidate ranking after existing
credential, avoidance, provider-only, and grade filters. It cannot make an
otherwise ineligible candidate executable.

## TDD sequence

Each slice starts with a focused failing unit or route-service test, records the
observed failure, applies the minimum production change, and reruns that test
before proceeding. Cross-cutting implementation follows only after the pure
selection and parsing contracts are covered.

1. Strict identity and exact-offering admission.
2. Provider-order tie-breaking.
3. OAuth environment credential classification.
4. Retry-delay metadata parsing and scheduling.
5. Model-level tool/reasoning gates.
6. Adapter-bounded manifest endpoint construction and selected/native identity.

## Compatibility and rollout

Failing closed can expose previously accepted typoed model names. The failure
must preserve the active route and provide a bounded diagnostic. Existing
canonical built-in routes and declared provider aliases remain valid.

Manifest endpoint execution widens only the sessionless route-lease schema with
optional endpoint ID, adapter ID, and inventory generation fields. Existing
sessionless records remain readable when those fields are absent. The frozen
session `route.lease_recorded` v1 payload remains unchanged. A new
`route.endpoint_provenance_recorded` v1 fact links the three endpoint fields to
the lease ID. Session-idle compaction uses the corresponding
`compaction.endpoint_provenance_recorded` v1 fact because it does not create a
turn route lease. A manifest-backed route records the host adapter contribution
generation. Selected offering identity remains distinct from the native serving
model. Native aliasing is not provider fallback.
