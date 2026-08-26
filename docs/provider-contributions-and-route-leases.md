+++
id = "provider-contributions-and-route-leases"
kind = "document"
title = "Provider contributions and route leases"
status = "implemented"
tags = ["providers", "routing", "authentication", "leases", "runtime"]
aliases = ["provider-route-authority"]
imported_reference = false

[publication]
enabled = false
visibility = "private"
+++

# Provider contributions and route leases

This is the canonical current guide to Omegon's provider-contribution,
provider-routing, and route-evidence boundary. It describes implemented Slice 4
behavior, not a provider catalog or a promise that every configured or
discovered offering is executable.

## Authorities

- `core/crates/omegon/src/provider_contributions.rs` owns validated executable
  contribution declarations and directed fallback compatibility.
- `core/crates/omegon/src/inference_inventory.rs` owns layered endpoint,
  offering, modality, capability, and provenance evidence.
- `core/crates/omegon/src/provider_route_service.rs` owns request route
  resolution and route-lease recording.
- `core/crates/omegon/src/route.rs` owns interactive model intent,
  `RouteController`, and selected-versus-serving route state.
- `core/crates/omegon/src/session_authority.rs` owns the session-backed
  `route.lease_recorded` fact and its reduction.
- [Runtime session semantic protocol](runtime-session-semantic-protocol.md)
  defines authority-stream ordering, snapshots, and Slice 5 limits.
- [Provider route conceptual model matrix](provider-route-conceptual-model-matrix.md)
  defines the broader identity and inventory model; this guide identifies the
  implemented subset.
- The normative OpenSpec delta is
  [provider routing: route contributions and leases](../openspec/archive/2026-08-26-selective-kernel-decomposition/specs/provider-routing/leases.md).

## Provider contributions

A provider contribution is release-coupled executable metadata under one stable
contribution owner and generation. A complete contribution binds:

- canonical provider identity and aliases;
- the runtime-inference-inventory authority for that provider;
- accepted authentication class;
- supported tool-schema dialect, or an explicit unsupported-tools state;
- a host-local typed bridge-factory identity;
- required offering-level modality and capability evidence semantics; and
- explicit directed, model-family-bounded fallback relations.

Validation rejects incomplete declarations, duplicate identities, mismatched
inventory ownership, incompatible factory/authentication pairs, and dangling or
self-referential fallback targets. Factory identities are metadata resolved by
the integration crate, not serialized function pointers. A valid contribution
does not assert that every provider offering exists, is healthy, has
credentials, or is suitable for a request.

## Inventory, evidence, and execution

Inventory and executable semantics are different layers:

- Runtime inventory composes endpoint/deployment and offering records with
  provenance-bearing modality and capability evidence.
- Contribution validation establishes that Omegon understands how to construct
  and normalize a provider route.
- Credential resolution establishes whether the route can presently obtain an
  accepted credential or use its declared local/credentialless posture.
- Route resolution chooses an executable bridge and records the evidence used
  at the dispatch boundary.

The contribution registry can diagnose missing offerings, modalities, or
capabilities in an inventory snapshot. Those diagnostics exist, but the current
dispatch path does not consume them as an eligibility gate. Documentation must
not claim that inventory evidence presently blocks provider dispatch. Likewise,
inventory presence alone never supplies a bridge factory or makes a route
executable.

## Identity and selection

Keep these identities distinct:

- provider identity names the administrative/transport integration;
- endpoint or deployment identity names a callable deployment;
- offering identity names a model exposed by that deployment;
- conceptual model identity links reviewed semantic equivalence across routes;
- contribution owner and generation identify the executable declaration;
- authentication class states which credential mechanisms a contribution
  accepts;
- credential-source class records the evidence available for this dispatch;
- selected provider/model records operator or caller intent; and
- serving provider/model records the bridge that receives the request.

Selected and serving identities are equal on a direct route. They differ on
fallback and are both retained by compatibility bridge handles and route leases.
Never rewrite a fallback route as though the operator selected the serving
provider.

Model intent may select a provider-neutral grade/provider policy or carry an
exact concrete model override. An exact model override is a pinned intent until
cleared. `GradePolicy::Exact` means exact requested grade, not necessarily an
exact concrete provider/model route. At the route-service boundary,
`resolve_exact` probes only the selected provider; ordinary sessionless
`resolve` may follow declared compatibility.

## Fallback and compatibility

Interactive startup fallback is opt-in. `fallbackProviders` is an ordered scope
for `RouteController` startup resolution; an empty list does not authorize an
implicit provider substitution and can leave the route disconnected.

That interactive policy is distinct from sessionless route-service resolution.
Sessionless callers using ordinary, non-exact resolution may follow fallback
relations declared by the selected provider contribution. They do not gain an
interactive session or inherit `fallbackProviders` merely because they use the
same route service.

Declared compatibility is directed and non-transitive. If A declares B, that
does not imply B declares A or that A may reach C through B. Sharing a wire
protocol is not compatibility evidence. Current validation checks the selected
model ID against the source contribution's declared model family and retains
that model ID on the serving provider. Do not promise broader serving-model
family, offering, capability, or quality validation than that implemented
selected-family check.

## Retry, fallback, and unknown invocation

These are separate states:

- A provider-request retry repeats request handling on the captured serving
  route. It is not permission to select another provider and is not a replay of
  a completed tool invocation.
- Fallback resolves a different declared-compatible serving provider and must
  preserve selected-versus-serving identity plus a bounded reason.
- An unknown invocation is a tool/privileged-invocation durability state after
  owner handoff where completion cannot be proved. It is governed by invocation
  leases, idempotency, deduplication, and mutation fences, not provider fallback.

No provider retry or fallback proves that an unknown invocation is safe to
replay.

## Authentication evidence

The contribution authentication class is a compatibility constraint such as
API key, OAuth, API-key-or-OAuth, token exchange, or a local credential posture.
It is not a secret, login endpoint, or claim about the credential selected for
one request.

The route lease's `credential_source_class` is bounded evidence about how the
serving bridge was resolved, for example an environment, stored, external, or
secrets-manager source class. When more specific source evidence is unavailable,
the implementation may record the contribution's authentication class. A lease
therefore always carries credential/authentication-class evidence, but does not
contain secret material and must not be interpreted as proof of one exhaustive
credential-resolution order.

See [Provider credential map](provider-credential-map.md) for the credential
authority boundaries, not an exhaustive provider or endpoint inventory.

## Route lease durability

Every provider stream records a versioned lease before dispatch. The lease
contains lease/request identity, selected and serving provider/model identity,
schema dialect or unsupported state, credential-source/authentication-class
evidence, bounded fallback reason, contribution generation, and route policy.
Current contribution generation and declared fallback compatibility are
revalidated before persistence. A stale generation, undeclared fallback,
partial session scope, or persistence failure prevents the provider call.

For session-backed work, the active session authority appends
`route.lease_recorded` for the active `turn_id`. The fact is part of the
authoritative session JSONL stream and is reduced into snapshot
`route_leases`. A stale turn or duplicate lease identity is rejected.

For sessionless work, the route service records the same lease shape inside a
versioned step wrapper in the durable `runtime/route-leases.jsonl` stream under
the Omegon home. The wrapper has an ephemeral `step_id` and timestamp; it does
not fabricate `session_id` or `turn_id`. This file is route evidence for
sessionless steps, not the complete Slice 5 semantic step stream.

There is no operator command for listing historical route leases. Do not
document a lease-history command or imply that current route/status projections
enumerate these append-only records.

## Slice 5 limits

Slice 4 records the minimum route fact needed to explain a dispatch. It does not
yet provide complete semantic persistence for model-context provenance,
assistant message and stream facts, tool calls/results, continuation and step
boundaries, compaction, provider-history derivation, or projection-specific
evolution. `AgentEvent`, route/status views, and existing conversation files
remain projections or compatibility records unless the session protocol says
otherwise. Route leases must not be presented as complete replay of a request or
turn.
