# Task capsule artifact design

## Artifact identity

`task-capsule` is initially a marker feature. It identifies one supported Cargo
composition built with `--no-default-features --features task-capsule`. The
marker is mutually exclusive with `tui`, `self-update`, and `local-embeddings`,
so an accidental larger build fails instead of claiming the exact capsule identity.

Composition inspection derives `task-capsule-v0` from compiled features. A
caller cannot make the normal product claim capsule identity by selecting a
profile string, and a capsule cannot claim interactive, daemon, or full product
composition. Compatibility validation runs before settings and `AgentSetup`, so
an incompatible request cannot create workspace or runtime state before refusal.

## First subtraction

The default product retains self-update. The `self-update` feature owns Sigstore
and X.509 certificate parsing. Capsule builds omit that feature and preserve a
stable update function that returns explicit compiled-capability unavailability
before network, filesystem, or process mutation.

Archive extraction remains linked in v0 because version switching and extension
installation share it. The retained-domain inventory must not imply otherwise.

## Execution contract

The canonical entrypoint is `omegon run`. V0 reuses the existing bounded runner,
provider routing, secrets, task specification, structured result, cancellation,
timeout, and exit-code contracts. It does not fork or duplicate the agent loop.
Canonical does not mean exclusive in V0: other non-TUI CLI commands remain linked
until command-surface fencing is implemented as a separate subtraction.

## Ratchet

A dedicated dependency check evaluates the exact capsule feature graph. Every
future subtraction extends its forbidden package set. Retained domains move to
the forbidden set only after their runtime behavior has a tested absence
contract. CI compile-checks and lints that graph, proves unsupported feature
combinations fail, release-builds it in an isolated target directory, and
black-box checks identity plus pre-start profile refusal.
Some presentation packages may also be transitive dependencies of retained V0
domains. In particular, `unicode-width` remains reachable through web parsing
and non-TUI truncation. The ratchet therefore verifies direct TUI ownership for
that crate rather than claiming package-level absence.

## Deferred layers

- A capsule-specific archive and container image.
- Integration into the five-profile full-product release and packaging matrix.
- Command-surface fencing beyond the canonical `run` entrypoint.
- Embedded control plane and ACP subtraction.
- Dynamic contribution, Git, memory, and lifecycle subtraction.
- A separately named executable after the shared loop boundary is extracted.
