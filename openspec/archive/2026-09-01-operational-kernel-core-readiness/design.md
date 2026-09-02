# Operational kernel and core readiness design

## Milestone shape

This is one readiness campaign with independently testable lanes. Each behavior
starts with a failing scenario-owned test, lands the smallest production change,
and records focused evidence before the next behavior is marked complete. The
campaign is PR-ready only as a whole. Intermediate slices may be committed and
reviewed, but they do not satisfy the milestone gate.

## Kernel execution sequence

The first slice adds a deterministic, no-network conformance turn to the current
reduced host. It makes the host's existing `system:default-loop`, `agent-loop`,
and `bounded-task` claims executable while preserving its dependency boundary.
This probe is not an admitted production provider and must identify itself as
scripted conformance evidence.

The production slice then extracts the smallest provider-neutral loop, session,
route, context, and invocation contracts needed by both the full product and the
reduced host. A real bounded kernel turn must use the same route authority and
terminal semantics as `omegon run`. It must not add a second provider factory,
session truth, or privileged invocation path to `kernel_host.rs`.

## Core classification

A checked machine-readable inventory classifies every first-party domain as one
of: constitutional resident, host service, signed core component, shipped
content, or operator-managed SDK extension. Each row records the current owner,
runtime boundary, extraction disposition, and evidence. Classification is a
decision gate, not a mandate to turn inexpensive or authority-bearing code into
a process.

Any domain promoted to `core:*` must add portable contracts, signed component
identity, kernel absence, additive restoration, full-product retention,
lifecycle cleanup, and aggregate budget evidence. `core:codescan` remains the
reference implementation.

## Resolved architecture decisions

- Fresh direct installation requires an external bootstrap verifier. A checksum
  cannot grant authenticity.
- The supervisor publication coordinator initiates changed-generation commit
  after an explicit quiescence proof. Turn completion does not initiate commit.
- Each contribution has at most one pending candidate. A newer candidate first
  settles and replaces the older pending candidate.
- Every edge consumes one versioned activity projection. One-shot CLI declares
  persistent busy-state reconciliation unsupported.
- Remote cleanup remains typed best-effort or unverified. This milestone does
  not add a remote acknowledgement protocol.
- Homebrew, Nix, and OCI scope is pre-publication policy and deterministic
  packaging verification. Live channel acceptance and publication remain outside
  this milestone.
- Stable tag pushes do not publish deferred package channels. Homebrew is
  manual-only, and release workflow dispatch must opt in explicitly before
  triggering Homebrew or OCI publication.

## Trust boundaries

Release archives remain the canonical composition evidence. Direct installation
and version switching must verify equivalent signed evidence before activation.
They must not downgrade to checksum-only success when verification support or
evidence is absent. A future Nix release lane may delegate artifact authenticity
to an exact pinned derivation and signed binary-cache policy, but its host-only
composition must remain explicit. A future OCI release lane must publish and
verify an image digest, SBOM, provenance, and composition identity before it is
considered supported.

A fresh direct install uses a required external bootstrap verifier. The
installer authenticates the archive, package manifest, bundle, release identity,
target, and composition before it extracts executable bytes. A checksum remains
an integrity aid and cannot replace authenticity. If the verifier or evidence is
missing, the installer fails closed and removes its staging directory.

An existing installation uses the maintenance executable from the currently
active, already verified generation to verify a switch candidate. The candidate
cannot verify itself. Verification completes before publication, and activation
continues to use one atomic generation selector for the host, maintenance
companion, required components, content, receipt, and locks. Failure preserves
the active selector and removes candidate staging.

OCI scope in this milestone is policy and deterministic fixture verification.
The verifier must reject an image candidate unless its signature, SBOM,
provenance, composition identity, and host-only or full-product class bind the
same image digest. Running a live release lane and producing or publishing those
attestations are outside this milestone.

## Core-component qualification

The composition policy will contain one generic qualification record for each
signed `core:*` component. The record binds every required boundary to a named
executor. These boundaries include contracts, signed identity, artifact ladder,
policy, protocol, cleanup, budgets, archive inventory, and SDK non-promotion.
A missing, duplicate, stale, or cross-component executor fails qualification.
Codescan is the first record and reference fixture. The gate must not hard-code
codescan as the only possible component.

## Runtime parity and activation

One semantic fixture matrix will feed TUI, ACP, Web, IPC, CLI, and daemon
adapters. Transport-specific serialization, redaction, and declared unsupported
bindings are permitted. Duplicated availability or admission policy is not.

The matrix uses one versioned, renderer-neutral activity projection. It carries
session and composition identity, a monotonic activity revision, queue state,
active-turn identity, terminal state, lifecycle health, canonical actions,
owner, and denial reason. An adapter can omit a field only through a checked
transport-limitation declaration. One-shot CLI declares persistent busy-state
reconciliation unsupported, but it still projects representable state and action
descriptors. Daemon projection consumes the shared DTO rather than reconstructing
Web state.

Activity caches compare revisions within the same session and composition
generation. A stale queue observation cannot override a newer durable closure.
Supervisor completion and authoritative idle snapshots release local active
gates without `AgentEnd`. Delayed or duplicate terminal advice is idempotent, and
every persistent edge must admit a second prompt after reconciliation.

Bounded task manifests become admitted execution policy. Time, turn, token, and
tool budgets are checked before the next governed action and settle with a typed
bounded outcome. Newly installed extension bytes enter discovery as a candidate
generation and may publish only at an idle, quiescent boundary. Failed staging
leaves the active generation unchanged.

Changed extension generations use digest-bound contribution-generation IDs. The
runtime retains one active generation and at most one staged candidate for a
contribution. If generation C arrives while B is pending behind active A, the
lifecycle owner settles all B resources before staging C. A supervisor-owned
publication coordinator performs an explicit commit transaction after it proves
the runtime is idle and has no unresolved invocation authority. Turn closure and
the next turn start do not publish a candidate by implication.

Publication atomically swaps the accepted graph, declarations, aliases, handles,
and transport owner. Old invocation leases and polling handles consult the same
generation fence and fail before owner entry. Fresh work captures only the new
generation. Failed staging or publication leaves A callable and exposes none of
the candidate's schemas, routes, actions, aliases, or processes.

Remote cleanup does not add a settlement protocol in this milestone. The host
must settle every resource it owns, then classify remote state as best-effort or
unverified. A timeout or transport close cannot be reported as strict remote
cleanup success.

## Implementation ownership

- Generic core qualification belongs in
  `fixtures/release-composition-matrix-v1.json` and
  `scripts/check_composition_matrix.py`, with mutation tests in
  `tests/test_composition_release_gates.py`.
- Fresh-install bootstrap verification and staging cleanup belong in
  `core/install.sh` and `core/install.test.sh`. Exact release and composition
  policy remains owned by `omegon-maintain` release verification.
- Switch admission belongs in `core/crates/omegon/src/switch.rs`. Atomic
  generation publication and recovery remain in `installed_release.rs`.
- OCI evidence policy belongs in the distribution-policy checker, deterministic
  fixtures, and CI readiness workflow. It does not belong in runtime startup.
- The versioned activity DTO belongs under `core/crates/omegon/src/surfaces/`.
  TUI, ACP, Web, IPC, CLI, and daemon code remain transport adapters.
- Changed-generation candidate ownership belongs in
  `contribution_lifecycle.rs`. Explicit commit coordination belongs in
  `runtime_supervisor.rs`. Accepted graph and lease fences remain in `bus.rs` and
  `invocation_service.rs`.
- Native extension handles remain transport adapters in `extensions/`, but must
  consult shared generation admission before `rpc_call_with_cancel`.
- Remote cleanup classification belongs in shared lifecycle projections. Each
  transport adapter reports only the resources and remote evidence it can
  observe.

## Acceptance corpus

`fixtures/operational-kernel-core-corpus-v1.json` is the milestone's scenario
catalog. Stable IDs join OpenSpec requirements to contract, policy,
state-machine, real-process, artifact, distribution, and platform evidence. The
catalog uses a constrained sentinel matrix rather than a Cartesian product.

Evidence status is explicit. `planned` evidence contributes to coverage design
but cannot satisfy a promotion profile. Separate profiles gate the scripted
kernel baseline, provider-backed kernel, signed core components, SDK addons, and
the complete milestone. This separation prevents a scripted provider, SDK
manifest, or source-only test from claiming stronger runtime or package
authority. The full rationale and extension procedure are in
`docs/operational-kernel-core-test-corpus.md`.

## Documentation impact

The kernel execution and domain-classification lanes update
`docs/binary-composition-and-kernel-admission.md`. Runtime parity and extension
activation update the relevant runtime architecture documents and
`site/src/pages/docs/extensions.astro`. Distribution trust updates
`site/src/pages/docs/install.astro`, `site/src/pages/docs/security.astro`, and
canonical install/verification snippets. Bounded-run behavior updates
`site/src/pages/docs/commands.astro` and `site/src/pages/docs/sessions.astro`.

The initial scripted conformance probe is a release/developer acceptance surface,
not a supported operator workflow, so it has no standalone public-site command.
Its exact output and limitation are documented in the durable composition guide.

## PR readiness

PR readiness requires every checkbox in `tasks.md`, all scenario-focused tests,
OpenSpec validation, `archive --check`, documentation validation, the applicable
composition/distribution gates, `just test-commit`, and `just clippy-changed`.
Archive-check is evidence that the completed deltas can merge cleanly. Actual
archival remains a deliberate lifecycle action after implementation verification.
